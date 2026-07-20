use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

use maud::{Markup, html};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::spacetimedb::Settlement;

const WIDTH: f64 = 1200.0;
const HEIGHT: f64 = 800.0;
const PACKAGE_JSON: &str = include_str!("../static/map/strategic-map-v1.json");

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
    package_sha256: String,
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
static GEOMETRY: OnceLock<GeometryPath> = OnceLock::new();

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
        package
    })
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
    let view_width = 390.0;
    let view_height = 260.0;
    let view_box = format!(
        "{:.2} {:.2} {view_width} {view_height}",
        origin_x - view_width / 2.0,
        origin_y - view_height / 2.0
    );
    let geometry = GEOMETRY.get_or_init(|| geometry_path(package));

    html! {
        section class="strategic-map" data-strategic-map data-map-theme="atlas"
            data-origin-x=(format!("{origin_x:.3}")) data-origin-y=(format!("{origin_y:.3}"))
            aria-label=(format!("Map around {}", current.map_or("the current settlement", |item| item.name.as_str()))) {
            div class="strategic-map-toolbar" role="toolbar" aria-label="Map controls" {
                div class="strategic-map-theme" aria-label="Map appearance" {
                    button type="button" class="map-theme-button" data-map-theme-choice="paper" aria-pressed="false" { "Paper" }
                    button type="button" class="map-theme-button" data-map-theme-choice="atlas" aria-pressed="true" { "Atlas" }
                }
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
                    rect class="map-land" x="0" y="0" width=(WIDTH) height=(HEIGHT) {}
                    @for (band, path) in &geometry.elevation {
                        path class=(format!("map-elevation map-elevation-{band}")) d=(path) {}
                    }
                    path class="map-water" d=(&geometry.water) fill-rule="evenodd" {}
                    @for ((kind, density), path) in &geometry.forest {
                        path class=(format!("map-forest map-forest-{kind} map-forest-density-{density}")) d=(path) {}
                    }
                    path class="map-forest-coverage" d=(&geometry.forest_coverage) {}
                    @for (elevation, path) in &geometry.contours {
                        path class=(format!("map-contour map-contour-{elevation}")) d=(path) {}
                    }
                    path class="map-road map-road-land" d=(&geometry.land) {}
                    path class="map-road map-road-ferry" d=(&geometry.ferry) {}
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

struct GeometryPath {
    land: String,
    ferry: String,
    water: String,
    elevation: BTreeMap<i16, String>,
    contours: BTreeMap<i16, String>,
    forest: BTreeMap<(String, u8), String>,
    forest_coverage: String,
}

fn geometry_path(package: &Package) -> GeometryPath {
    let mut geometry = GeometryPath {
        land: String::new(),
        ferry: String::new(),
        water: String::new(),
        elevation: BTreeMap::new(),
        contours: BTreeMap::new(),
        forest: BTreeMap::new(),
        forest_coverage: String::new(),
    };
    for cell in &package.elevation.cells {
        append_bounds(
            geometry.elevation.entry(cell.band_m).or_default(),
            cell.bounds,
            package.bounds,
        );
    }
    for line in &package.elevation.contours {
        append_path(
            geometry.contours.entry(line.elevation_m).or_default(),
            &line.points,
            false,
            package.bounds,
        );
    }
    for region in &package.forest.regions {
        append_bounds(
            geometry
                .forest
                .entry((region.kind.clone(), region.density))
                .or_default(),
            region.bounds,
            package.bounds,
        );
    }
    for bounds in &package.forest.coverage {
        append_bounds(&mut geometry.forest_coverage, *bounds, package.bounds);
    }
    for line in &package.roads {
        append_path(
            if line.kind == "ferry" {
                &mut geometry.ferry
            } else {
                &mut geometry.land
            },
            &line.points,
            false,
            package.bounds,
        );
    }
    for ring in &package.water {
        append_path(&mut geometry.water, ring, true, package.bounds);
    }
    geometry
}

fn append_bounds(output: &mut String, [west, south, east, north]: [f64; 4], bounds: [f64; 4]) {
    let points = [[west, north], [east, north], [east, south], [west, south]];
    append_path(output, &points, true, bounds);
}

fn append_path(output: &mut String, points: &[[f64; 2]], close: bool, bounds: [f64; 4]) {
    for (index, point) in points.iter().enumerate() {
        let (x, y) = project(point[0], point[1], bounds);
        output.push_str(if index == 0 { "M" } else { "L" });
        output.push_str(&format!("{x:.2},{y:.2}"));
    }
    if close {
        output.push('Z');
    }
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
    fn svg_has_two_themes_accessible_controls_and_canonical_pin_links() {
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

        assert!(markup.contains("data-map-theme-choice=\"paper\""));
        assert!(markup.contains("data-map-theme-choice=\"atlas\""));
        assert!(markup.contains("aria-label=\"Zoom map in\""));
        assert!(markup.contains("tabindex=\"0\""));
        assert!(markup.contains("?destination=near"));
        assert!(markup.contains("Nearby, direct route available"));
        assert!(markup.contains("Far away, no direct route"));
        assert!(!markup.contains("?destination=demo"));
        assert!(!markup.contains("role=\"img\""));
        assert!(markup.contains("aria-current=\"true\""));
        assert!(markup.contains("map-pin-selection"));
        assert!(markup.contains("map-elevation-"));
        assert!(markup.contains("map-forest-"));
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
    fn immutable_geometry_is_cached_and_bounded() {
        let first = GEOMETRY.get_or_init(|| geometry_path(package()));
        let second = GEOMETRY.get_or_init(|| geometry_path(package()));
        assert!(std::ptr::eq(first, second));
        let bytes = first.land.len()
            + first.ferry.len()
            + first.water.len()
            + first.forest_coverage.len()
            + first.elevation.values().map(String::len).sum::<usize>()
            + first.contours.values().map(String::len).sum::<usize>()
            + first.forest.values().map(String::len).sum::<usize>();
        assert!(
            bytes < 4_000_000,
            "formatted SVG geometry grew to {bytes} bytes"
        );
    }
}
