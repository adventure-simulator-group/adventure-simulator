use std::{
    collections::{BTreeSet, HashMap},
    ops::Range,
    path::Path,
    sync::Arc,
};

use adventuresim_world_schema::coordinates::Wgs84CoordinateE7;
use axum::{
    body::{Body, Bytes},
    extract::{Query, State},
    http::{
        Response, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
};
use maud::{Markup, html};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[cfg(test)]
use crate::spacetimedb::DestinationKnowledgeStage;
use crate::spacetimedb::{BackendCaseSitePin, SettlementCategory, SettlementView};
use crate::templates::prosperity_tier_label;

const WIDTH: f64 = 1200.0;
const HEIGHT: f64 = 800.0;
const VIEW_ASPECT_RATIO: f64 = WIDTH / HEIGHT;
const DEFAULT_VIEW_WIDTH: f64 = 390.0;
const DEFAULT_VIEW_HEIGHT: f64 = DEFAULT_VIEW_WIDTH / VIEW_ASPECT_RATIO;
const ROUTE_MIN_VIEW_WIDTH: f64 = 90.0;
const ROUTE_MIN_VIEW_HEIGHT: f64 = ROUTE_MIN_VIEW_WIDTH / VIEW_ASPECT_RATIO;
pub(crate) const TILE_PATH_PREFIX: &str = "/map/tiles/";
pub(crate) const DATA_LICENSE_PATH: &str = "/map/data-license";
const DATA_LICENSE: &str = include_str!("../../../MAP_DATA_LICENSE.md");
const PACKAGE_SCHEMA: u32 = 5;
const RENDERER_REVISION: u32 = 10;
const MIN_TILE_SIZE: u32 = 64;
const MAX_TILE_SIZE: u32 = 2_048;
const MAX_TILE_ENTRIES: usize = 100_000;
const ANNUAL_IMMUTABLE_TILE_CACHE_SECONDS: u32 = 31_536_000;
const VIABUNDUS_URL: &str = "https://doi.org/10.5281/zenodo.16611998";
const ELEVATION_URL: &str = "https://doi.org/10.5270/ESA-c5d3d65";
const FOREST_URL: &str = "https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a";

#[derive(Debug, Deserialize)]
struct Package {
    schema: u32,
    renderer_revision: u32,
    bounds: [f64; 4],
    source: Source,
    elevation: ElevationLayer,
    forest: ForestLayer,
    cultivation: CultivationLayer,
    tiles: TilePyramid,
    terrain_package_sha256: String,
    inferred_road_geometry_sha256: String,
    wetland_source_sha256: String,
    package_sha256: String,
}

#[derive(Debug, Deserialize)]
struct TilePyramid {
    format: String,
    tile_size: u32,
    gutter: u8,
    max_zoom: u8,
    content_sha256: String,
    entries: Vec<TileEntry>,
}

#[derive(Debug, Deserialize)]
struct TileEntry {
    theme: String,
    zoom: u8,
    x: u16,
    y: u16,
    offset: u64,
    length: u32,
}

#[derive(Debug, Deserialize)]
struct Source {
    url: String,
}

#[derive(Debug, Deserialize)]
struct LayerSource {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ElevationLayer {
    source: LayerSource,
}

#[derive(Debug, Deserialize)]
struct ForestLayer {
    source: LayerSource,
}

#[derive(Debug, Deserialize)]
struct CultivationLayer {
    grid_crs: String,
    grid_resolution_m: u16,
    rules_version: u16,
    source_sha256: String,
    square_count: usize,
}

pub struct StrategicMap {
    package: Package,
    tile_pack: Arc<[u8]>,
    tile_index: HashMap<String, Range<usize>>,
}

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

impl StrategicMap {
    pub fn load(bundle_dir: &Path) -> anyhow::Result<Self> {
        let package_path = bundle_dir.join("strategic-map-v1.json");
        let tile_path = bundle_dir.join("strategic-map-tiles-v1.pack");
        let package_json = std::fs::read_to_string(package_path)?;
        let package: Package = serde_json::from_str(&package_json)?;
        anyhow::ensure!(
            package.schema == PACKAGE_SCHEMA,
            "unsupported strategic map package schema"
        );
        for digest in [
            &package.terrain_package_sha256,
            &package.inferred_road_geometry_sha256,
            &package.wetland_source_sha256,
            &package.cultivation.source_sha256,
        ] {
            anyhow::ensure!(
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                "invalid strategic map dependency digest"
            );
        }
        anyhow::ensure!(
            package.renderer_revision == RENDERER_REVISION,
            "unsupported strategic map renderer revision"
        );
        anyhow::ensure!(
            package.cultivation.grid_crs == "EPSG:3035"
                && package.cultivation.grid_resolution_m == 1_000
                && package.cultivation.rules_version > 0
                && package.cultivation.square_count <= 5_000_000,
            "invalid strategic map cultivation contract"
        );
        anyhow::ensure!(
            package_json_digest(&package_json, &package.package_sha256)? == package.package_sha256,
            "strategic map package digest mismatch"
        );
        anyhow::ensure!(
            package.source.url == VIABUNDUS_URL
                && package.elevation.source.url == ELEVATION_URL
                && package.forest.source.url == FOREST_URL,
            "strategic map source URL mismatch"
        );
        anyhow::ensure!(
            package.tiles.format == "avif",
            "unsupported map tile format"
        );
        anyhow::ensure!(
            (MIN_TILE_SIZE..=MAX_TILE_SIZE).contains(&package.tiles.tile_size)
                && package.tiles.tile_size.is_power_of_two(),
            "invalid map tile size"
        );
        anyhow::ensure!(
            u32::from(package.tiles.gutter) < package.tiles.tile_size / 4,
            "invalid map tile gutter"
        );
        anyhow::ensure!(package.tiles.max_zoom <= 8, "map tile zoom exceeds bound");
        let expected_entries =
            pyramid_tile_entry_count(package.tiles.tile_size, package.tiles.max_zoom);
        anyhow::ensure!(
            expected_entries <= MAX_TILE_ENTRIES,
            "map tile pyramid exceeds entry bound"
        );
        anyhow::ensure!(
            !package.tiles.entries.is_empty() && package.tiles.entries.len() <= expected_entries,
            "map tile pyramid is empty or oversized"
        );
        let tile_pack: Arc<[u8]> = std::fs::read(tile_path)?.into();
        anyhow::ensure!(
            format!("{:x}", Sha256::digest(&tile_pack)) == package.tiles.content_sha256,
            "strategic map tile-pack digest mismatch"
        );
        let mut tile_index = HashMap::with_capacity(package.tiles.entries.len());
        for entry in &package.tiles.entries {
            anyhow::ensure!(entry.theme == "paper", "unsupported map tile theme");
            let Some((columns, rows)) = tile_grid_size(package.tiles.tile_size, entry.zoom) else {
                anyhow::bail!("map tile entry zoom exceeds package bound");
            };
            anyhow::ensure!(
                entry.zoom <= package.tiles.max_zoom
                    && u32::from(entry.x) < columns
                    && u32::from(entry.y) < rows,
                "map tile entry coordinate is outside its grid"
            );
            let start = usize::try_from(entry.offset)?;
            let end = start
                .checked_add(entry.length as usize)
                .ok_or_else(|| anyhow::anyhow!("map tile entry overflows pack bounds"))?;
            anyhow::ensure!(end <= tile_pack.len(), "map tile entry exceeds pack bounds");
            anyhow::ensure!(
                tile_index
                    .insert(
                        tile_key(&entry.theme, entry.zoom, entry.x, entry.y),
                        start..end
                    )
                    .is_none(),
                "duplicate map tile entry"
            );
        }
        Ok(Self {
            package,
            tile_pack,
            tile_index,
        })
    }

    pub fn validate_terrain_identity(
        &self,
        terrain: &adventuresim_terrain::TerrainPack,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            terrain_identity_matches(
                &self.package,
                terrain.purpose(),
                terrain.package_sha256(),
                terrain.wetland_source_sha256(),
                terrain.cultivation_source_sha256(),
                terrain.bounds()
            ),
            "map/terrain purpose, package, wetland, or bounds identity mismatch"
        );
        Ok(())
    }
}

fn terrain_identity_matches(
    package: &Package,
    purpose: adventuresim_terrain::TerrainPurpose,
    terrain_sha: &str,
    wetland_sha: &str,
    cultivation_sha: &str,
    bounds: [f64; 4],
) -> bool {
    purpose == adventuresim_terrain::TerrainPurpose::Final
        && terrain_sha == package.terrain_package_sha256
        && wetland_sha == package.wetland_source_sha256
        && cultivation_sha == package.cultivation.source_sha256
        && bounds == package.bounds
}

fn tile_grid_size(tile_size: u32, zoom: u8) -> Option<(u32, u32)> {
    (zoom <= 8).then(|| {
        let scale = 1_u32 << zoom;
        (
            (WIDTH as u32 * scale).div_ceil(tile_size),
            (HEIGHT as u32 * scale).div_ceil(tile_size),
        )
    })
}

fn pyramid_tile_entry_count(tile_size: u32, max_zoom: u8) -> usize {
    (0..=max_zoom)
        .filter_map(|zoom| tile_grid_size(tile_size, zoom))
        .map(|(columns, rows)| columns as usize * rows as usize)
        .sum()
}

#[derive(Deserialize)]
pub(crate) struct TileQuery {
    v: Option<String>,
}

pub(crate) async fn data_license() -> Response<Body> {
    Response::builder()
        .header(CONTENT_TYPE, "text/markdown; charset=utf-8")
        .header(CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(DATA_LICENSE))
        .expect("map data licence response")
}

pub(crate) async fn world_tile(
    State(state): State<crate::routes::AppState>,
    axum::extract::Path((theme, zoom, x, tile)): axum::extract::Path<(String, u8, u16, String)>,
    Query(query): Query<TileQuery>,
) -> Response<Body> {
    let Some(map) = state.strategic_map.as_deref() else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .expect("map unavailable response");
    };
    let Some(y) = tile
        .strip_suffix(".avif")
        .and_then(|value| value.parse::<u16>().ok())
    else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .expect("static tile response");
    };
    let package = &map.package;
    let Some(range) = map.tile_index.get(&tile_key(&theme, zoom, x, y)) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .expect("static tile response");
    };
    let bytes = Bytes::from_owner(map.tile_pack.clone()).slice(range.clone());
    let mut response = Response::builder().header(CONTENT_TYPE, "image/avif");
    if query.v.as_deref() == Some(package.tiles.content_sha256.as_str()) {
        response = response.header(
            CACHE_CONTROL,
            format!("public, max-age={ANNUAL_IMMUTABLE_TILE_CACHE_SECONDS}, immutable"),
        );
    }
    response
        .body(Body::from(bytes))
        .expect("static tile response")
}

fn tile_key(theme: &str, zoom: u8, x: u16, y: u16) -> String {
    format!("{theme}/{zoom}/{x}/{y}")
}

#[cfg(test)]
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

pub(crate) fn has_geographic_source(settlement: &SettlementView) -> bool {
    settlement.source_node_id.is_some()
        && settlement.longitude.is_finite()
        && settlement.latitude.is_finite()
}

fn has_geographic_case_site(site: &BackendCaseSitePin) -> bool {
    site.coordinates_are_geographic
}

fn case_site_longitude_latitude(site: &BackendCaseSitePin) -> Option<(f64, f64)> {
    Wgs84CoordinateE7::new(site.latitude_e_7, site.longitude_e_7)
        .map(Wgs84CoordinateE7::longitude_latitude_degrees)
}

fn has_geographic_source_in_bounds(settlement: &SettlementView, bounds: [f64; 4]) -> bool {
    has_geographic_source(settlement)
        && adventuresim_world_schema::coordinates_in_bounds(
            settlement.longitude,
            settlement.latitude,
            bounds,
        )
}

fn has_geographic_case_site_in_bounds(site: &BackendCaseSitePin, bounds: [f64; 4]) -> bool {
    has_geographic_case_site(site)
        && case_site_longitude_latitude(site).is_some_and(|(longitude, latitude)| {
            adventuresim_world_schema::coordinates_in_bounds(longitude, latitude, bounds)
        })
}

fn settlement_symbol_kind(category: &SettlementCategory) -> &'static str {
    match category {
        SettlementCategory::City | SettlementCategory::Capital => "city",
        SettlementCategory::Town => "town",
        SettlementCategory::Unknown | SettlementCategory::Hamlet | SettlementCategory::Village => {
            "village"
        }
    }
}

fn settlement_label_priority(
    settlement: &SettlementView,
    is_current: bool,
    is_connected: bool,
    is_selected: bool,
) -> u16 {
    if is_current {
        return 120;
    }
    if is_selected {
        return 115;
    }
    let category_priority = match &settlement.category {
        SettlementCategory::Capital => 80,
        SettlementCategory::City => 70,
        SettlementCategory::Town => 60,
        SettlementCategory::Village => 50,
        SettlementCategory::Hamlet => 40,
        SettlementCategory::Unknown => 30,
    };
    category_priority + u16::from(is_connected) * 25
}

fn population_level_threshold(view_width: f64) -> i32 {
    if view_width > 900.0 {
        5
    } else if view_width > 600.0 {
        4
    } else if view_width > 350.0 {
        3
    } else if view_width > 180.0 {
        2
    } else {
        1
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the map renderer consumes independent overlay, selection, and route layers"
)]
pub fn strategic_map(
    map: &StrategicMap,
    settlements: &[SettlementView],
    case_sites: &[BackendCaseSitePin],
    current_id: &str,
    connected_ids: &BTreeSet<&str>,
    selected_id: Option<&str>,
    map_path: &str,
    terrain_route: Option<&adventuresim_terrain::RoutePlan>,
) -> Markup {
    let package = &map.package;
    let current = settlements.iter().find(|settlement| {
        settlement.id == current_id && has_geographic_source_in_bounds(settlement, package.bounds)
    });
    let (origin_x, origin_y) = current.map_or((WIDTH / 2.0, HEIGHT / 2.0), |settlement| {
        project(settlement.longitude, settlement.latitude, package.bounds)
    });
    let settlement_destination = selected_id
        .and_then(|selected_id| {
            settlements.iter().find(|settlement| {
                settlement.id == selected_id
                    && has_geographic_source_in_bounds(settlement, package.bounds)
            })
        })
        .map(|settlement| project(settlement.longitude, settlement.latitude, package.bounds));
    let case_site_destination = selected_id
        .and_then(|selected_id| {
            case_sites.iter().find(|site| {
                site.case_site_id.value == selected_id
                    && has_geographic_case_site_in_bounds(site, package.bounds)
            })
        })
        .map(|site| {
            let (longitude, latitude) = case_site_longitude_latitude(site)
                .expect("in-bounds case site must have valid WGS84 coordinates");
            project(longitude, latitude, package.bounds)
        });
    let destination = settlement_destination.or(case_site_destination);
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
            div class="strategic-map-controls" role="group" aria-label="Map controls" {
                button type="button" class="strategic-map-control" data-map-action="zoom-in"
                    aria-label="Zoom in" data-strategic-tooltip="Zoom in" { span aria-hidden="true" { "+" } }
                button type="button" class="strategic-map-control" data-map-action="zoom-out"
                    aria-label="Zoom out" data-strategic-tooltip="Zoom out" { span aria-hidden="true" { "−" } }
                button type="button" class="strategic-map-control" data-map-action="reset"
                    aria-label="Reset map view" data-strategic-tooltip="Reset map view" { span aria-hidden="true" { "↺" } }
            }
            svg class="strategic-map-svg" data-map-svg viewBox=(view_box)
                aria-labelledby="strategic-map-title strategic-map-description" tabindex="0" {
                title id="strategic-map-title" { "Settlement road map" }
                desc id="strategic-map-description" { "Use arrow keys or drag to pan. Use plus, minus, or the mouse wheel to zoom. Activate a settlement or quest pin to inspect it." }
                defs {
                    g id="map-settlement-village-symbol" {
                        path class="map-settlement-ground" d="M-10,3 Q0,5 10,3" {}
                        path class="map-settlement-shape" d="M-6,3 V-5 L0,-10 L6,-5 V3 Z" {}
                        path class="map-settlement-door" d="M-1.5,3 V-1 H1.5 V3 Z" {}
                    }
                    g id="map-settlement-town-symbol" {
                        path class="map-settlement-ground" d="M-10,3 Q0,5 10,3" {}
                        path class="map-settlement-shape" d="M-8,3 V-5 H-6 V-10 H-3 V-5 H2 V-12 H5 V-5 H8 V3 Z" {}
                        path class="map-settlement-door" d="M-1.7,3 V-1.5 Q0,-4 1.7,-1.5 V3 Z" {}
                    }
                    g id="map-settlement-city-symbol" {
                        path class="map-settlement-ground" d="M-10,3 Q0,5 10,3" {}
                        path class="map-settlement-shape" d="M-9,3 V-6 H-7 V-11 H-4 V-6 H-2 V-14 H2 V-6 H4 V-11 H7 V-6 H9 V3 Z" {}
                        path class="map-settlement-door" d="M-2,3 V-2 Q0,-5 2,-2 V3 Z" {}
                    }
                }
                g data-map-viewport {
                    g class="map-tile-layer" data-map-tile-layer aria-hidden="true" {
                        @for (x, y, left, top) in initial_tiles {
                            image x=(format!("{:.3}", left - tile_gutter)) y=(format!("{:.3}", top - tile_gutter))
                                width=(format!("{:.3}", tile_span + 2.0 * tile_gutter)) height=(format!("{:.3}", tile_span + 2.0 * tile_gutter))
                                preserveAspectRatio="none"
                                href=(tile_url("paper", initial_tile_zoom, x, y, &package.tiles.content_sha256)) {}
                        }
                    }
                    rect class="map-atmosphere-layer" x="0" y="0" width="1200" height="800" aria-hidden="true" {}
                    g class="map-overlay-layer" {
                        @if let Some(route) = terrain_route {
                            @let points = route.points.iter().map(|point| { let (x,y)=project(point.longitude, point.latitude, package.bounds); format!("{x:.3},{y:.3}") }).collect::<Vec<_>>().join(" ");
                            polyline class="map-selection-line map-terrain-route" data-map-selection-line
                                aria-label=(format!("Computed terrain route, {:.1} kilometres", route.distance_m as f64 / 1000.0))
                                points=(points) {}
                        } @else if let Some((destination_x, destination_y)) = destination {
                            line class="map-selection-line map-straight-line-route" data-map-selection-line aria-hidden="true"
                                x1=(format!("{origin_x:.3}")) y1=(format!("{origin_y:.3}"))
                                x2=(format!("{destination_x:.3}")) y2=(format!("{destination_y:.3}")) {}
                        }
                        @for settlement in settlements.iter().filter(|settlement| has_geographic_source_in_bounds(settlement, package.bounds)) {
                            @let (x, y) = project(settlement.longitude, settlement.latitude, package.bounds);
                            @let is_current = settlement.id == current_id;
                            @let is_connected = connected_ids.contains(settlement.id.as_str());
                            @let is_selected = selected_id == Some(settlement.id.as_str());
                            @let is_essential = is_current || is_selected;
                            @let initially_visible = is_essential || settlement.population_level >= population_level_threshold(initial_view.width);
                            @let symbol_kind = settlement_symbol_kind(&settlement.category);
                            @let label_priority = settlement_label_priority(settlement, is_current, is_connected, is_selected);
                            @let label_width = (settlement.name.chars().count() as u16 * 7 + 8).clamp(44, 180);
                            @let prosperity = prosperity_tier_label(settlement.economy.prosperity_tier);
                            @let label = if is_current { format!("{}, {prosperity} prosperity, current settlement", settlement.name) } else if is_connected { format!("{}, {prosperity} prosperity, direct route available", settlement.name) } else { format!("{}, {prosperity} prosperity, no direct route", settlement.name) };
                            a href=(format!("{map_path}?destination={}", settlement.id))
                                class="map-pin-link" aria-label=(&label) data-strategic-tooltip=(&label) aria-current=[is_selected.then_some("true")]
                                data-map-pin data-map-settlement data-settlement-id=(&settlement.id) data-connected=(is_connected)
                                data-map-population-level=(settlement.population_level) data-map-pin-essential=(is_essential)
                                display=[(!initially_visible).then_some("none")] {
                                g class=(format!("map-pin map-settlement map-settlement-{symbol_kind}{}{}{}", if is_current { " current" } else { "" }, if is_connected { " connected" } else { "" }, if is_selected { " selected" } else { "" }))
                                    transform=(format!("translate({x:.3} {y:.3})")) {
                                    g data-map-pin-symbol transform=(format!("scale({initial_pin_scale:.5})")) {
                                        circle class="map-settlement-hit-area" r="13" {}
                                        @if is_selected { circle class="map-pin-selection" cy="-3" r="12" {} }
                                        use class="map-settlement-pictogram" aria-hidden="true"
                                            href=(format!("#map-settlement-{symbol_kind}-symbol")) {}
                                        @if is_current { path class="map-pin-current-mark" d="M0,-15 V-22 M0,-22 L7,-19 L0,-16 Z" {} }
                                        @if is_selected { path class="map-pin-selected-mark" d="M-5,13 H5" {} }
                                        title { (&settlement.name) }
                                    }
                                    g class="map-settlement-label" data-map-label
                                        data-map-x=(format!("{x:.3}")) data-map-y=(format!("{y:.3}"))
                                        data-map-label-priority=(label_priority) data-map-label-width=(label_width)
                                        data-map-label-essential=((is_current || is_selected).to_string()) {
                                        text class="map-settlement-label-text" x="0" y="0" { (&settlement.name) }
                                    }
                                }
                            }
                        }
                        @for site in case_sites.iter().filter(|site| has_geographic_case_site_in_bounds(site, package.bounds)) {
                            @let (longitude, latitude) = case_site_longitude_latitude(site).expect("in-bounds case site must have valid WGS84 coordinates");
                            @let (x, y) = project(longitude, latitude, package.bounds);
                            @let is_selected = selected_id == Some(site.case_site_id.value.as_str());
                            @let label = format!("Known case site: {}", site.name);
                            a href=(format!("{map_path}?destination={}", site.case_site_id.value))
                                class="map-pin-link map-quest-link" aria-label=(&label) data-strategic-tooltip=(&label)
                                aria-current=[is_selected.then_some("true")]
                                data-map-pin data-case-site-id=(&site.case_site_id.value) {
                                g class=(format!("map-pin map-quest active{}", if is_selected { " selected" } else { "" }))
                                    transform=(format!("translate({x:.3} {y:.3})")) {
                                    g data-map-pin-symbol transform=(format!("scale({initial_pin_scale:.5})")) {
                                        circle class="map-quest-hit-area" r="13" {}
                                        @if is_selected { circle class="map-pin-selection" r="12" {} }
                                        path class="map-quest-shape" d="M0,-9 L9,0 L0,9 L-9,0 Z" {}
                                        path class="map-quest-mark" d="M0,-5 V2 M0,5 V6" {}
                                        @if is_selected { path class="map-pin-selected-mark" d="M-5,13 H5" {} }
                                        title { (&site.name) }
                                    }
                                }
                            }
                        }
                        // Settlement symbols take pointer priority when a generated quest happens
                        // to overlap them. The visible/accessibility link remains above; this
                        // transparent duplicate is mouse-only and is rendered last in SVG order.
                        @for settlement in settlements.iter().filter(|settlement| has_geographic_source_in_bounds(settlement, package.bounds)) {
                            @let (x, y) = project(settlement.longitude, settlement.latitude, package.bounds);
                            @let is_essential = settlement.id == current_id || selected_id == Some(settlement.id.as_str());
                            @let initially_visible = is_essential || settlement.population_level >= population_level_threshold(initial_view.width);
                            a href=(format!("{map_path}?destination={}", settlement.id))
                                class="map-settlement-hit-link" aria-hidden="true" tabindex="-1"
                                data-map-settlement-hit data-map-population-level=(settlement.population_level)
                                data-map-pin-essential=(is_essential) display=[(!initially_visible).then_some("none")] {
                                g transform=(format!("translate({x:.3} {y:.3})")) {
                                    g data-map-pin-symbol transform=(format!("scale({initial_pin_scale:.5})")) {
                                        circle class="map-settlement-hit-area map-settlement-hit-overlay" r="13" {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
            a class="strategic-map-license-link" href=(DATA_LICENSE_PATH)
                rel="license" target="_blank" { "Map data licence" }
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

pub fn strategic_map_bundle_unavailable() -> Markup {
    html! {
        section class="strategic-map strategic-map-unavailable" role="status" aria-labelledby="map-bundle-unavailable-title" {
            h2 id="map-bundle-unavailable-title" { "Map layer unavailable" }
            p { "The optional offline map bundle is not installed. Destination selection and direct travel remain available in the surrounding HTML interface." }
        }
    }
}

fn package_json_digest(json: &str, expected: &str) -> anyhow::Result<String> {
    anyhow::ensure!(
        expected.len() == 64
            && expected
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "strategic map digest must be lowercase SHA-256"
    );
    let marker = format!("\"package_sha256\":\"{expected}\"");
    anyhow::ensure!(
        json.matches(&marker).count() == 1,
        "strategic map digest field must occur exactly once"
    );
    let unsigned = json.trim_end_matches(['\r', '\n']).replace(
        &marker,
        &format!("\"package_sha256\":\"{}\"", "0".repeat(64)),
    );
    let bytes = unsigned.as_bytes();
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn project(longitude: f64, latitude: f64, [west, south, east, north]: [f64; 4]) -> (f64, f64) {
    let x = ((longitude - west) / (east - west) * WIDTH).clamp(0.0, WIDTH);
    let y = ((north - latitude) / (north - south) * HEIGHT).clamp(0.0, HEIGHT);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static BUNDLE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn test_bundle_contents() -> (String, Vec<u8>) {
        let tile_pack = b"\0\0\0\x18ftypavif".to_vec();
        let tile_digest = format!("{:x}", Sha256::digest(&tile_pack));
        let placeholder = "0".repeat(64);
        let unsigned = format!(
            concat!(
                r#"{{"schema":4,"renderer_revision":3,"year":1544,"bounds":[-10.0,40.0,30.0,70.0],"source":{{"name":"Test roads","url":"https://doi.org/10.5281/zenodo.16611998","license":"CC0","verification_status":"verified"}},"elevation":{{"source":{{"name":"Test elevation","url":"https://doi.org/10.5270/ESA-c5d3d65","file_count":1}}}},"forest":{{"source":{{"name":"Test forest","url":"https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a","file_count":1}},"coverage_tiles":1}},"cultivation":{{"grid_crs":"EPSG:3035","grid_resolution_m":1000,"rules_version":1,"source_sha256":"{}","square_count":1}},"tiles":{{"format":"avif","tile_size":2048,"gutter":4,"max_zoom":0,"content_sha256":"{}","entries":[{{"theme":"paper","zoom":0,"x":0,"y":0,"offset":0,"length":12}}]}},"terrain_package_sha256":"{}","inferred_road_geometry_sha256":"{}","wetland_source_sha256":"{}","package_sha256":"{}"}}"#
            ),
            placeholder, tile_digest, placeholder, placeholder, placeholder, placeholder
        )
        .replace("\"schema\":4", &format!("\"schema\":{PACKAGE_SCHEMA}"))
        .replace(
            "\"renderer_revision\":3",
            &format!("\"renderer_revision\":{RENDERER_REVISION}"),
        );
        let package_digest = format!("{:x}", Sha256::digest(unsigned.as_bytes()));
        (
            unsigned.replace(
                &format!(r#""package_sha256":"{placeholder}""#),
                &format!(r#""package_sha256":"{package_digest}""#),
            ),
            tile_pack,
        )
    }

    fn sign_manifest(mut value: serde_json::Value) -> String {
        let placeholder = "0".repeat(64);
        value["package_sha256"] = placeholder.clone().into();
        let unsigned = serde_json::to_string(&value).unwrap();
        let digest = format!("{:x}", Sha256::digest(unsigned.as_bytes()));
        unsigned.replace(
            &format!(r#""package_sha256":"{placeholder}""#),
            &format!(r#""package_sha256":"{digest}""#),
        )
    }

    fn load_rewritten_manifest(
        rewrite: impl FnOnce(&mut serde_json::Value),
    ) -> anyhow::Result<StrategicMap> {
        let (manifest, tile_pack) = test_bundle_contents();
        let mut value: serde_json::Value = serde_json::from_str(&manifest).unwrap();
        rewrite(&mut value);
        let manifest = sign_manifest(value);
        let root = write_test_bundle(&manifest, &tile_pack);
        let loaded = StrategicMap::load(&root);
        std::fs::remove_dir_all(&root).expect("remove rewritten test map bundle");
        loaded
    }

    fn write_test_bundle(manifest: &str, tile_pack: &[u8]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "adventuresim-strategic-map-test-{}-{}",
            std::process::id(),
            BUNDLE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).expect("create test map bundle");
        std::fs::write(root.join("strategic-map-v1.json"), manifest)
            .expect("write test map manifest");
        std::fs::write(root.join("strategic-map-tiles-v1.pack"), tile_pack)
            .expect("write test tile pack");
        root
    }

    fn map_bundle() -> StrategicMap {
        let (manifest, tile_pack) = test_bundle_contents();
        let root = write_test_bundle(&manifest, &tile_pack);
        let loaded = StrategicMap::load(&root);
        std::fs::remove_dir_all(&root).expect("remove test map bundle");
        loaded.expect("load test map bundle")
    }

    #[test]
    fn terrain_identity_requires_final_matching_package_wetland_and_bounds() {
        let map = map_bundle();
        let terrain = map.package.terrain_package_sha256.clone();
        let wet = map.package.wetland_source_sha256.clone();
        let cultivation = map.package.cultivation.source_sha256.clone();
        let bounds = map.package.bounds;
        assert!(terrain_identity_matches(
            &map.package,
            adventuresim_terrain::TerrainPurpose::Final,
            &terrain,
            &wet,
            &cultivation,
            bounds
        ));
        assert!(!terrain_identity_matches(
            &map.package,
            adventuresim_terrain::TerrainPurpose::DocumentedBase,
            &terrain,
            &wet,
            &cultivation,
            bounds
        ));
        assert!(!terrain_identity_matches(
            &map.package,
            adventuresim_terrain::TerrainPurpose::Final,
            &"f".repeat(64),
            &wet,
            &cultivation,
            bounds
        ));
        assert!(!terrain_identity_matches(
            &map.package,
            adventuresim_terrain::TerrainPurpose::Final,
            &terrain,
            &"e".repeat(64),
            &cultivation,
            bounds
        ));
    }

    fn settlement(id: &str, name: &str, longitude: f64, latitude: f64) -> SettlementView {
        SettlementView {
            id: id.into(),
            name: name.into(),
            longitude,
            latitude,
            population_level: 4,
            population_estimate: 1_000,
            category: crate::spacetimedb::SettlementCategory::Town,
            languages: adventuresim_world_schema::SettlementLanguageProfile {
                east_central_bp: 10_000,
                west_central_bp: 0,
                low_bp: 0,
                yiddish_incidence_bp: 75,
            },
            industries: adventuresim_world_schema::InferredIndustryProfile::new(vec![
                adventuresim_world_schema::IndustryEvidence::Fallback(
                    adventuresim_world_schema::FallbackIndustry::CroplandGrain,
                ),
            ])
            .unwrap(),
            economy: adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder(),
            religious_status: adventuresim_world_schema::SettlementReligiousStatus::Established {
                religion: adventuresim_world_schema::OfficialReligion::RomanCatholic,
            },
            scene_key: "hills".into(),
            religion_id: "western_church".into(),
            currency_id: "coin".into(),
            source_node_id: Some(1),
        }
    }

    fn case_site(id: &str, title: &str, longitude: f64, latitude: f64) -> BackendCaseSitePin {
        let coordinate =
            Wgs84CoordinateE7::from_longitude_latitude_degrees(longitude, latitude).unwrap();
        BackendCaseSitePin {
            owner_character_id: 7,
            case_id: "quest-1".into(),
            case_site_id: adventuresim_stdb_client::CaseSiteId { value: id.into() },
            origin_settlement_id: "origin".into(),
            name: title.into(),
            description: "A camp in the woods.".into(),
            scene_key: "forest".into(),
            longitude_e_7: coordinate.longitude().get(),
            latitude_e_7: coordinate.latitude().get(),
            coordinates_are_geographic: true,
            distance_m: 8_000,
            knowledge_stage: DestinationKnowledgeStage::ExactBelieved,
            tracked: true,
            display_title: title.into(),
            generated_case: false,
            case_resolved: false,
            combat_available: false,
            opposition_count: None,
            opposition_combat_power: None,
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
    fn population_pin_threshold_reveals_smaller_places_as_the_camera_closes() {
        assert_eq!(population_level_threshold(1_200.0), 5);
        assert_eq!(population_level_threshold(900.0), 4);
        assert_eq!(population_level_threshold(600.0), 3);
        assert_eq!(population_level_threshold(350.0), 2);
        assert_eq!(population_level_threshold(180.0), 1);
    }

    #[test]
    fn tiled_map_has_canonical_pin_links_and_accessible_controls() {
        let map = map_bundle();
        let mut source_less = settlement("demo", "Demo", 0.0, 0.0);
        source_less.source_node_id = None;
        let settlements = [
            settlement("origin", "Origin", 10.0, 53.0),
            settlement("near", "Nearby", 11.0, 53.2),
            settlement("far", "Far away", 20.0, 60.0),
            settlement("outside", "Outside package", 40.0, 80.0),
            source_less,
        ];
        let connected = BTreeSet::from(["near"]);
        let markup = strategic_map(
            &map,
            &settlements,
            &[],
            "origin",
            &connected,
            Some("near"),
            "/locations/settlement/origin/map",
            None,
        )
        .into_string();

        assert!(markup.contains("data-map-theme=\"paper\""));
        assert!(!markup.contains("data-map-theme-choice"));
        assert!(!markup.contains("strategic-map-toolbar"));
        assert!(!markup.contains("data-map-zoom"));
        assert_eq!(markup.matches("class=\"strategic-map-control\"").count(), 3);
        assert!(markup.contains("data-map-action=\"zoom-in\""));
        assert!(markup.contains("data-map-action=\"zoom-out\""));
        assert!(markup.contains("data-map-action=\"reset\""));
        assert!(markup.contains("data-strategic-tooltip=\"Zoom in\""));
        assert!(markup.contains("tabindex=\"0\""));
        assert!(markup.contains("?destination=near"));
        assert!(markup.contains("Nearby,"));
        assert!(markup.contains("direct route available"));
        assert!(markup.contains("data-strategic-tooltip=\"Nearby,"));
        assert!(markup.contains("Far away,"));
        assert!(markup.contains("no direct route"));
        assert!(!markup.contains("Outside package"));
        assert!(!markup.contains("?destination=demo"));
        assert!(!markup.contains("role=\"img\""));
        assert!(markup.contains("aria-current=\"true\""));
        assert!(markup.contains("map-pin-selection"));
        assert!(markup.contains("data-map-pin-symbol"));
        assert!(markup.contains("map-settlement-town"));
        assert!(markup.contains("data-map-label-priority"));
        assert!(markup.contains("map-settlement-label-text"));
        assert!(markup.contains("map-tile-layer"));
        assert!(markup.contains("map-atmosphere-layer"));
        assert!(markup.contains("map-overlay-layer"));
        assert!(markup.contains("data-map-selection-line"));
        assert!(markup.contains(TILE_PATH_PREFIX));
        assert!(markup.contains(&map.package.tiles.content_sha256));
        assert!(markup.contains("image"));
        assert!(markup.contains("class=\"map-settlement-hit-area\" r=\"13\""));
        assert!(!markup.contains("class=\"map-elevation"));
        assert!(
            markup.len() < 50_000,
            "dynamic map markup grew to {} bytes",
            markup.len()
        );
        assert!(!markup.contains("strategic-map-legend"));
        assert!(!markup.contains("strategic-map-attribution"));
        assert!(!markup.contains("Map data:"));
        assert!(markup.contains("href=\"/map/data-license\""));
        assert!(markup.contains("rel=\"license\""));
        assert!(DATA_LICENSE.contains("Creative Commons Attribution-ShareAlike 4.0"));
        assert!(DATA_LICENSE.contains("The organisations in charge of the Copernicus programme"));
    }

    #[test]
    fn selected_case_site_has_a_pin_and_computed_terrain_route() {
        let map = map_bundle();
        let site = case_site("site:opaque-hash", "The abandoned croft", 11.0, 53.2);
        let route = adventuresim_terrain::RoutePlan {
            points: vec![
                adventuresim_terrain::RoutePoint {
                    latitude: 53.0,
                    longitude: 10.0,
                },
                adventuresim_terrain::RoutePoint {
                    latitude: 53.1,
                    longitude: 10.4,
                },
                adventuresim_terrain::RoutePoint {
                    latitude: 53.2,
                    longitude: 11.0,
                },
            ],
            spans: Vec::new(),
            distance_m: 74_500,
            minutes: 1_100,
        };
        let markup = strategic_map(
            &map,
            &[settlement("origin", "Origin", 10.0, 53.0)],
            std::slice::from_ref(&site),
            "origin",
            &BTreeSet::new(),
            Some("site:opaque-hash"),
            "/locations/settlement/origin/map",
            Some(&route),
        )
        .into_string();

        assert!(markup.contains("data-case-site-id=\"site:opaque-hash\""));
        assert!(markup.contains("?destination=site:opaque-hash"));
        assert!(markup.contains("Known case site: The abandoned croft"));
        assert!(!markup.contains("Known case site: site:opaque-hash"));
        assert!(markup.contains("map-quest-shape"));
        assert!(markup.contains("data-map-selection-line"));
        assert!(markup.contains("map-terrain-route"));
        assert!(!markup.contains("map-straight-line-route"));
        assert!(markup.contains("Computed terrain route, 74.5 kilometres"));
        assert!(markup.contains("aria-current=\"true\""));
        assert!(markup.contains("map-settlement-hit-overlay"));
        assert!(
            markup.rfind("map-settlement-hit-overlay").unwrap()
                > markup.rfind("map-quest-shape").unwrap(),
            "settlement pointer overlay must render after quest pins"
        );
    }

    #[test]
    fn settlement_labels_prioritize_state_and_population_class() {
        let mut village = settlement("village", "Village", 10.0, 53.0);
        village.category = SettlementCategory::Village;
        let mut capital = settlement("capital", "Capital", 10.0, 53.0);
        capital.category = SettlementCategory::Capital;

        assert_eq!(settlement_symbol_kind(&village.category), "village");
        assert_eq!(settlement_symbol_kind(&capital.category), "city");
        assert_eq!(settlement_label_priority(&village, false, false, false), 50);
        assert_eq!(settlement_label_priority(&capital, false, false, false), 80);
        assert_eq!(settlement_label_priority(&village, false, true, false), 75);
        assert_eq!(settlement_label_priority(&village, false, false, true), 115);
        assert_eq!(settlement_label_priority(&village, true, false, false), 120);
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
    fn package_identity_covers_raw_presentation_content() {
        let (json, _) = test_bundle_contents();
        let package: Package = serde_json::from_str(&json).expect("test map metadata");
        let original = package_json_digest(&json, &package.package_sha256).unwrap();
        assert_eq!(original, package.package_sha256);

        let changed = json.replacen("\"year\":1544", "\"year\":1545", 1);
        assert_ne!(
            original,
            package_json_digest(&changed, &package.package_sha256).unwrap()
        );
    }

    #[test]
    fn base_map_tile_urls_are_content_versioned() {
        let map = map_bundle();
        assert_eq!(map.tile_index.len(), 1);
        let markup = strategic_map(
            &map,
            &[settlement("origin", "Origin", 10.0, 53.0)],
            &[],
            "origin",
            &BTreeSet::new(),
            None,
            "/locations/settlement/origin/map",
            None,
        )
        .into_string();
        let package = &map.package;
        let entry = package.tiles.entries.first().expect("generated tile index");
        let path = format!(
            "{TILE_PATH_PREFIX}{}/{}/{}/{}.avif",
            entry.theme, entry.zoom, entry.x, entry.y
        );
        assert!(markup.contains("data-map-tile-version"));
        assert!(markup.contains("data-map-tile-gutter=\"4\""));
        assert!(markup.contains(&format!("?v={}", package.tiles.content_sha256)));
        assert_eq!(
            tile_coordinates(&path),
            Some((entry.theme.as_str(), entry.zoom, entry.x, entry.y))
        );
        assert_eq!(tile_coordinates("/map/tiles/paper/0/0/0.png"), None);
        let start = usize::try_from(entry.offset).unwrap();
        let end = start + entry.length as usize;
        assert!(
            map.tile_pack[start..end]
                .windows(8)
                .any(|bytes| bytes == b"ftypavif")
        );
    }

    #[test]
    fn missing_runtime_bundle_is_nonfatal_and_has_html_fallback() {
        let missing = Path::new("definitely-missing-map-artifact");
        assert!(StrategicMap::load(missing).is_err());
        let markup = strategic_map_bundle_unavailable().into_string();
        assert!(markup.contains("Map layer unavailable"));
        assert!(markup.contains("Destination selection and direct travel remain available"));
    }

    #[test]
    fn malformed_runtime_manifest_returns_an_error_instead_of_panicking() {
        let malformed = r#"{"package_sha256":"short"}"#;
        assert!(package_json_digest(malformed, "short").is_err());
        let repeated = format!(
            "{{\"package_sha256\":\"{0}\",\"copy\":{{\"package_sha256\":\"{0}\"}}}}",
            "0".repeat(64)
        );
        assert!(package_json_digest(&repeated, &"0".repeat(64)).is_err());

        let (manifest, tile_pack) = test_bundle_contents();
        let package: Package = serde_json::from_str(&manifest).expect("test map metadata");
        let malformed = manifest.replace(&package.package_sha256, "short");
        let root = write_test_bundle(&malformed, &tile_pack);
        let loaded = StrategicMap::load(&root);
        std::fs::remove_dir_all(&root).expect("remove malformed test map bundle");
        assert!(loaded.is_err());
    }

    #[test]
    fn stale_renderer_revision_is_rejected() {
        assert!(load_rewritten_manifest(|value| value["renderer_revision"] = 0.into()).is_err());
    }

    #[test]
    fn runtime_rejects_undersized_or_excessive_tile_pyramids() {
        assert!(load_rewritten_manifest(|value| value["tiles"]["tile_size"] = 2.into()).is_err());
        assert!(
            load_rewritten_manifest(|value| {
                value["tiles"]["tile_size"] = MIN_TILE_SIZE.into();
                value["tiles"]["max_zoom"] = 8.into();
            })
            .is_err()
        );
    }

    #[test]
    fn runtime_rejects_tile_coordinates_outside_the_declared_grid() {
        assert!(
            load_rewritten_manifest(|value| value["tiles"]["entries"][0]["x"] = 1.into()).is_err()
        );
    }

    #[test]
    fn malicious_or_unexpected_source_urls_are_rejected() {
        for path in [
            ["source", "url"],
            ["elevation", "source"],
            ["forest", "source"],
        ] {
            let loaded = load_rewritten_manifest(|value| {
                if path[1] == "url" {
                    value[path[0]][path[1]] = "javascript:alert(1)".into();
                } else {
                    value[path[0]][path[1]]["url"] = "http://evil.example/source".into();
                }
            });
            assert!(loaded.is_err());
        }
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
