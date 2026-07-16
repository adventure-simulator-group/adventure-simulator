//! Copernicus EU-Hydro settlement-water and road-crossing enrichment.
//!
//! The reader consumes extracted basin GeoPackages in EPSG:3035. Raw source
//! sentinel values are resolved at this boundary; the canonical schema has no
//! unknown persistence, order, salinity, or route-waterway states.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    CanalWatercourse, CompiledWorld, CrossingTraversal, CrossingWatercourse, EdgeEndpoint,
    EdgeProgressPermille, FerryRoute, FerryWaterway, FlowPersistence, FlowingWaterAccess,
    InlandWaterAccess, InlandWaterSize, LandRoute, LandWaterCrossing, MarineWaterAccess,
    RiverAccess, RiverAndCanalAccess, RiverWatercourse, SettlementHydrology, SettlementImport,
    SourceProvenance, StrahlerOrder, TravelEdgeImport, TravelRoute, WORLD_SCHEMA_VERSION,
    WaterDistanceMeters, WorldMetadata,
};
use geo::{BoundingRect, Contains, Coord, Geometry, Line, LineString, Point};
use geozero::{ToGeo, wkb::GpkgWkb};
use proj4rs::{proj::Proj, transform::transform};
use rusqlite::{Connection, OpenFlags, params};

use crate::{
    Error, Result,
    draft::{
        DroughtSettlementDraft, SettlementDraftAccess, TravelEdgeDraft, TravelRouteDraft,
        WorldDraft, push_source_note,
    },
};

const SOURCE_NAME: &str = "Copernicus EU-Hydro River Network Database v1.3";
const SOURCE_URL: &str =
    "https://land.copernicus.eu/en/products/eu-hydro/eu-hydro-river-network-database";
const SOURCE_LICENSE: &str = "Copernicus Land Monitoring Service full and open data policy";
const EXPECTED_SRS: i64 = 3035;
const SETTLEMENT_ADJACENCY_METERS: f64 = 2_000.0;
const SOURCE_MARGIN_METERS: f64 = 10_000.0;
const CROSSING_DEDUP_METERS: f64 = 5.0;
const ENDPOINT_TOUCH_METERS: f64 = 5.0;
const INDEX_CELL_METERS: f64 = 10_000.0;

pub(crate) fn enrich(
    mut draft: WorldDraft<DroughtSettlementDraft>,
    source_directory: &Path,
) -> Result<CompiledWorld> {
    let projection = HydrologyProjection::new()?;
    let projected_nodes = draft
        .nodes
        .iter()
        .map(|node| Ok((node.id, projection.project(node.latitude, node.longitude)?)))
        .collect::<Result<HashMap<_, _>>>()?;
    let bounds = world_bounds(projected_nodes.values().copied());
    let database = HydrologyDatabase::open(source_directory, bounds)?;

    let mut landlocked_settlements = 0;
    let hydrology = draft
        .settlements
        .iter()
        .map(|settlement| {
            let base = base_settlement(settlement);
            let point = projected_nodes[&base.source_node_id];
            let value = database.sample_settlement(point)?;
            landlocked_settlements += usize::from(value == SettlementHydrology::default());
            Ok(value)
        })
        .collect::<Result<Vec<_>>>()?;

    let mut crossings = 0;
    let mut inferred_ferries = 0;
    let edges = std::mem::take(&mut draft.edges)
        .into_iter()
        .map(|mut edge| {
            let from = projected_nodes[&edge.from_node_id];
            let to = projected_nodes[&edge.to_node_id];
            let (route, edge_crossings, inferred) = database.enrich_route(edge.route, from, to)?;
            crossings += edge_crossings;
            inferred_ferries += usize::from(inferred);
            let note = match &route {
                TravelRoute::Land(route) if route.water_crossings.is_empty() => "**[EU-Hydro v1.3](https://doi.org/10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c):** No mapped linear watercourse intersects the straight endpoint-to-endpoint road geometry after endpoint-touch filtering.",
                TravelRoute::Land(_) => "**[EU-Hydro v1.3](https://doi.org/10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c):** Water crossings come from mapped road/linear-water intersections. Bridge versus ford is inferred deterministically from watercourse attributes unless Viabundus supplies endpoint bridge evidence; unmatched Viabundus bridge evidence receives the documented small-river fallback.",
                TravelRoute::Ferry(_) if inferred => "**EU-Hydro ferry fallback:** No mapped water feature explained this Viabundus ferry, so the route uses the documented plausible small perennial river waterway.",
                TravelRoute::Ferry(_) => "**[EU-Hydro v1.3](https://doi.org/10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c):** The ferry waterway is classified from a mapped river, inland-water, tidal-water, or coastal feature near the route.",
            };
            edge.sources.push('\n');
            edge.sources.push_str("- ");
            edge.sources.push_str(note);
            Ok(finish_edge(edge, route))
        })
        .collect::<Result<Vec<_>>>()?;

    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(hydrology)
        .map(|(mut drought, hydrology)| {
            push_source_note(
                &mut drought,
                if hydrology == SettlementHydrology::default() {
                    "**[EU-Hydro v1.3](https://doi.org/10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c):** No mapped flowing, inland, tidal, or coastal feature lies within the two-kilometer settlement-adjacency threshold; absence is treated as landlocked rather than unknown."
                } else {
                    "**[EU-Hydro v1.3](https://doi.org/10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c):** Flowing, inland, and marine access is derived by exact geometry distance within two kilometers. Missing/sentinel source attributes are resolved by documented parser-boundary defaults rather than stored as unknown."
                },
            );
            finish_settlement(drought, hydrology)
        })
        .collect::<Vec<_>>();

    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.hydrology_files_read = database.files_read;
    draft.report.hydrology_features_read = database.features.len();
    draft.report.hydrology_settlement_samples = settlements.len();
    draft.report.hydrology_landlocked_settlements = landlocked_settlements;
    draft.report.hydrology_edge_crossings = crossings;
    draft.report.hydrology_inferred_ferry_waterways = inferred_ferries;
    draft.report.route_crossings = edges
        .iter()
        .filter(|edge| edge.route.has_crossing())
        .count();

    Ok(CompiledWorld {
        metadata: WorldMetadata {
            schema_version: WORLD_SCHEMA_VERSION,
            world_year: draft.year,
            sources: draft.sources,
            road_types: draft.road_types,
        },
        nodes: draft.nodes,
        edges,
        settlement_aliases: draft.settlement_aliases,
        settlement_descriptions: draft.settlement_descriptions,
        settlements,
        report: draft.report,
    })
}

fn finish_edge(edge: TravelEdgeDraft, route: TravelRoute) -> TravelEdgeImport {
    TravelEdgeImport {
        id: edge.id,
        from_node_id: edge.from_node_id,
        to_node_id: edge.to_node_id,
        route,
        toll: edge.toll,
        length_m: edge.length_m,
        slope_multiplier: edge.slope_multiplier,
        certainty: edge.certainty,
        section: edge.section,
        sources: edge.sources,
    }
}

fn finish_settlement(
    drought: DroughtSettlementDraft,
    hydrology: SettlementHydrology,
) -> SettlementImport {
    let religious = drought.religious;
    let geologic = religious.geologic;
    let soil = geologic.soil;
    let trees = soil.trees;
    let vegetated = trees.vegetated;
    let forest = vegetated.forest;
    let land = forest.land;
    let elevated = land.elevated;
    let settlement = elevated.settlement;
    SettlementImport {
        id: settlement.id,
        source_node_id: settlement.source_node_id,
        name: settlement.name,
        longitude: settlement.longitude,
        latitude: settlement.latitude,
        population_level: settlement.population_level,
        population_estimate: settlement.population_estimate,
        elevation: elevated.elevation,
        land_use: land.land_use,
        forest_cover: forest.forest_cover,
        potential_vegetation: vegetated.potential_vegetation,
        tree_species: trees.tree_species,
        soil: soil.soil,
        geology: geologic.geology,
        religious_status: religious.religious_status,
        drought: drought.drought,
        hydrology,
        scene_key: settlement.scene_key,
        sources: settlement.sources,
    }
}

fn base_settlement(draft: &DroughtSettlementDraft) -> &crate::draft::SettlementDraft {
    draft.base_settlement()
}

struct HydrologyProjection {
    geographic: Proj,
    projected: Proj,
}

impl HydrologyProjection {
    fn new() -> Result<Self> {
        Ok(Self {
            geographic: Proj::from_proj_string(
                "+proj=longlat +datum=WGS84 +ellps=WGS84 +no_defs +type=crs",
            )?,
            projected: Proj::from_proj_string(
                "+proj=laea +lat_0=52 +lon_0=10 +x_0=4321000 +y_0=3210000 +ellps=GRS80 +units=m +no_defs +type=crs",
            )?,
        })
    }

    fn project(&self, latitude: f64, longitude: f64) -> Result<Point<f64>> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !(-180.0..=180.0).contains(&longitude)
        {
            return Err(Error::Validation(format!(
                "invalid coordinate ({latitude}, {longitude}) for EU-Hydro"
            )));
        }
        let mut coordinate = (longitude.to_radians(), latitude.to_radians(), 0.0);
        transform(&self.geographic, &self.projected, &mut coordinate)?;
        Ok(Point::new(coordinate.0, coordinate.1))
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

fn world_bounds(points: impl Iterator<Item = Point<f64>>) -> Option<Bounds> {
    points.fold(None, |bounds, point| {
        Some(match bounds {
            None => Bounds {
                min_x: point.x() - SOURCE_MARGIN_METERS,
                min_y: point.y() - SOURCE_MARGIN_METERS,
                max_x: point.x() + SOURCE_MARGIN_METERS,
                max_y: point.y() + SOURCE_MARGIN_METERS,
            },
            Some(bounds) => Bounds {
                min_x: bounds.min_x.min(point.x() - SOURCE_MARGIN_METERS),
                min_y: bounds.min_y.min(point.y() - SOURCE_MARGIN_METERS),
                max_x: bounds.max_x.max(point.x() + SOURCE_MARGIN_METERS),
                max_y: bounds.max_y.max(point.y() + SOURCE_MARGIN_METERS),
            },
        })
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FeatureKind {
    River,
    Canal,
    Ditch,
    InlandWater,
    Tidal,
    Coastal,
}

#[derive(Debug)]
struct WaterFeature {
    kind: FeatureKind,
    geometry: Geometry<f64>,
    order: StrahlerOrder,
    persistence: FlowPersistence,
    navigable: bool,
    area_square_meters: f64,
}

struct HydrologyDatabase {
    files_read: usize,
    features: Vec<WaterFeature>,
    spatial_grid: HashMap<(i32, i32), Vec<usize>>,
}

impl HydrologyDatabase {
    fn open(directory: &Path, bounds: Option<Bounds>) -> Result<Self> {
        if !directory.is_dir() {
            return Err(Error::MissingSource(directory.to_path_buf()));
        }
        let mut paths = Vec::new();
        collect_geopackages(directory, &mut paths)?;
        paths.sort();
        if paths.is_empty() {
            return Err(Error::Validation(format!(
                "{} contains no extracted EU-Hydro .gpkg files",
                directory.display()
            )));
        }
        let mut features = Vec::new();
        for path in &paths {
            read_geopackage(path, bounds, &mut features)?;
        }
        if !features
            .iter()
            .any(|feature| feature.kind == FeatureKind::River)
        {
            return Err(Error::Validation(
                "EU-Hydro source contains no River_Net_l features in the world extent".into(),
            ));
        }
        let spatial_grid = build_spatial_grid(&features);
        Ok(Self {
            files_read: paths.len(),
            features,
            spatial_grid,
        })
    }

    fn sample_settlement(&self, point: Point<f64>) -> Result<SettlementHydrology> {
        let candidates = self.candidates(Bounds {
            min_x: point.x() - SETTLEMENT_ADJACENCY_METERS,
            min_y: point.y() - SETTLEMENT_ADJACENCY_METERS,
            max_x: point.x() + SETTLEMENT_ADJACENCY_METERS,
            max_y: point.y() + SETTLEMENT_ADJACENCY_METERS,
        });
        let nearest = |kind| {
            candidates
                .iter()
                .copied()
                .filter(|feature| feature.kind == kind)
                .filter_map(|feature| {
                    geometry_distance(point, &feature.geometry)
                        .filter(|distance| *distance <= SETTLEMENT_ADJACENCY_METERS)
                        .map(|distance| (feature, distance))
                })
                .min_by(|left, right| left.1.total_cmp(&right.1))
        };
        let river = nearest(FeatureKind::River);
        let canal = nearest(FeatureKind::Canal);
        let flowing = river.map(|(river, distance)| {
            let river = RiverAccess {
                distance: water_distance(distance),
                order: river.order,
                persistence: river.persistence,
            };
            match canal {
                Some((canal, canal_distance)) => {
                    FlowingWaterAccess::RiverAndCanal(RiverAndCanalAccess {
                        river,
                        canal_distance: water_distance(canal_distance),
                        canal_navigable: canal.navigable,
                    })
                }
                None => FlowingWaterAccess::River(river),
            }
        });
        let inland =
            nearest(FeatureKind::InlandWater).map(|(feature, distance)| InlandWaterAccess {
                distance: water_distance(distance),
                size: inland_size(feature.area_square_meters),
            });
        let tidal = nearest(FeatureKind::Tidal);
        let coastal = nearest(FeatureKind::Coastal);
        let marine = match (tidal, coastal) {
            (Some((_, tidal)), Some((_, coastal))) if tidal <= coastal => {
                Some(MarineWaterAccess::Tidal(water_distance(tidal)))
            }
            (Some((_, tidal)), None) => Some(MarineWaterAccess::Tidal(water_distance(tidal))),
            (_, Some((_, coastal))) => Some(MarineWaterAccess::OpenCoast(water_distance(coastal))),
            (None, None) => None,
        };
        Ok(SettlementHydrology {
            flowing,
            inland,
            marine,
        })
    }

    fn enrich_route(
        &self,
        route: TravelRouteDraft,
        from: Point<f64>,
        to: Point<f64>,
    ) -> Result<(TravelRoute, usize, bool)> {
        let line = Line::new(from.0, to.0);
        match route {
            TravelRouteDraft::Land { bridge } => {
                let route_length = line_length(line);
                let mut crossings = self
                    .line_crossings(line)
                    .into_iter()
                    .filter(|crossing| {
                        (crossing.distance_from_start > ENDPOINT_TOUCH_METERS
                            || matches!(bridge, Some(EdgeEndpoint::From | EdgeEndpoint::Both)))
                            && (crossing.distance_to_end > ENDPOINT_TOUCH_METERS
                                || matches!(bridge, Some(EdgeEndpoint::To | EdgeEndpoint::Both)))
                    })
                    .collect::<Vec<_>>();
                apply_bridge_evidence(&mut crossings, bridge);
                add_unmapped_bridges(&mut crossings, bridge, route_length);
                sort_and_deduplicate_crossings(&mut crossings);
                let count = crossings.len();
                let crossings = crossings
                    .into_iter()
                    .map(|crossing| crossing.crossing)
                    .collect();
                Ok((
                    TravelRoute::Land(LandRoute {
                        bridge,
                        water_crossings: crossings,
                    }),
                    count,
                    false,
                ))
            }
            TravelRouteDraft::Ferry => {
                let midpoint = Point::new((from.x() + to.x()) / 2.0, (from.y() + to.y()) / 2.0);
                let crossing = self.line_crossings(line).into_iter().next();
                let waterway = crossing
                    .map(|crossing| ferry_from_crossing(crossing.crossing.watercourse))
                    .or_else(|| self.ferry_polygon(midpoint));
                let inferred = waterway.is_none();
                let waterway = waterway.unwrap_or(FerryWaterway::River(RiverWatercourse {
                    order: StrahlerOrder::new(2).expect("constant is valid"),
                    persistence: FlowPersistence::Perennial,
                }));
                Ok((TravelRoute::Ferry(FerryRoute { waterway }), 0, inferred))
            }
        }
    }

    fn line_crossings(&self, route: Line<f64>) -> Vec<LocatedCrossing> {
        let bounds = Bounds {
            min_x: route.start.x.min(route.end.x),
            min_y: route.start.y.min(route.end.y),
            max_x: route.start.x.max(route.end.x),
            max_y: route.start.y.max(route.end.y),
        };
        self.candidates(bounds)
            .iter()
            .copied()
            .filter(|feature| {
                matches!(
                    feature.kind,
                    FeatureKind::River | FeatureKind::Canal | FeatureKind::Ditch
                )
            })
            .flat_map(|feature| {
                geometry_line_intersections(route, &feature.geometry)
                    .into_iter()
                    .map(move |progress| {
                        let length = line_length(route);
                        LocatedCrossing {
                            crossing: LandWaterCrossing {
                                position: EdgeProgressPermille::new(
                                    (progress * EdgeProgressPermille::MAX as f64).round() as u16,
                                )
                                .expect("intersection is clamped"),
                                watercourse: feature_crossing(feature),
                                traversal: plausible_traversal(feature),
                            },
                            distance_from_start: progress * length,
                            distance_to_end: (1.0 - progress) * length,
                        }
                    })
            })
            .collect()
    }

    fn ferry_polygon(&self, point: Point<f64>) -> Option<FerryWaterway> {
        let candidates = self.candidates(Bounds {
            min_x: point.x(),
            min_y: point.y(),
            max_x: point.x(),
            max_y: point.y(),
        });
        [
            (FeatureKind::Tidal, FerryWaterway::TidalWater),
            (FeatureKind::Coastal, FerryWaterway::CoastalWater),
            (FeatureKind::InlandWater, FerryWaterway::InlandWater),
        ]
        .into_iter()
        .find_map(|(kind, waterway)| {
            candidates
                .iter()
                .copied()
                .any(|feature| feature.kind == kind && geometry_contains(&feature.geometry, point))
                .then_some(waterway)
        })
    }

    fn candidates(&self, bounds: Bounds) -> Vec<&WaterFeature> {
        let (min_x, max_x) = grid_range(bounds.min_x, bounds.max_x);
        let (min_y, max_y) = grid_range(bounds.min_y, bounds.max_y);
        let mut indices = HashSet::new();
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                if let Some(cell) = self.spatial_grid.get(&(x, y)) {
                    indices.extend(cell.iter().copied());
                }
            }
        }
        let mut indices = indices.into_iter().collect::<Vec<_>>();
        indices.sort_unstable();
        indices
            .into_iter()
            .map(|index| &self.features[index])
            .collect()
    }
}

struct LocatedCrossing {
    crossing: LandWaterCrossing,
    distance_from_start: f64,
    distance_to_end: f64,
}

fn line_length(line: Line<f64>) -> f64 {
    ((line.end.x - line.start.x).powi(2) + (line.end.y - line.start.y).powi(2)).sqrt()
}

fn build_spatial_grid(features: &[WaterFeature]) -> HashMap<(i32, i32), Vec<usize>> {
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (index, feature) in features.iter().enumerate() {
        let Some(rect) = feature.geometry.bounding_rect() else {
            continue;
        };
        let (min_x, max_x) = grid_range(rect.min().x, rect.max().x);
        let (min_y, max_y) = grid_range(rect.min().y, rect.max().y);
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                grid.entry((x, y)).or_default().push(index);
            }
        }
    }
    grid
}

fn grid_range(minimum: f64, maximum: f64) -> (i32, i32) {
    (
        (minimum / INDEX_CELL_METERS).floor() as i32,
        (maximum / INDEX_CELL_METERS).floor() as i32,
    )
}

fn water_distance(distance: f64) -> WaterDistanceMeters {
    WaterDistanceMeters::new(distance.round().clamp(0.0, WaterDistanceMeters::MAX as f64) as u16)
        .expect("distance is clamped")
}

fn inland_size(area: f64) -> InlandWaterSize {
    if area >= 100_000_000.0 {
        InlandWaterSize::GreatLake
    } else if area >= 1_000_000.0 {
        InlandWaterSize::Lake
    } else {
        InlandWaterSize::Pond
    }
}

fn feature_crossing(feature: &WaterFeature) -> CrossingWatercourse {
    match feature.kind {
        FeatureKind::River => CrossingWatercourse::River(RiverWatercourse {
            order: feature.order,
            persistence: feature.persistence,
        }),
        FeatureKind::Canal => CrossingWatercourse::Canal(CanalWatercourse {
            navigable: feature.navigable,
        }),
        FeatureKind::Ditch => CrossingWatercourse::Ditch,
        _ => unreachable!("only linear watercourses are crossings"),
    }
}

fn plausible_traversal(feature: &WaterFeature) -> CrossingTraversal {
    match feature.kind {
        FeatureKind::River if feature.order.get() <= 2 => CrossingTraversal::Ford,
        _ => CrossingTraversal::Bridge,
    }
}

fn ferry_from_crossing(crossing: CrossingWatercourse) -> FerryWaterway {
    match crossing {
        CrossingWatercourse::River(river) => FerryWaterway::River(river),
        CrossingWatercourse::Canal(_) | CrossingWatercourse::Ditch => {
            FerryWaterway::River(RiverWatercourse {
                order: StrahlerOrder::new(2).expect("constant is valid"),
                persistence: FlowPersistence::Perennial,
            })
        }
    }
}

fn apply_bridge_evidence(crossings: &mut [LocatedCrossing], bridge: Option<EdgeEndpoint>) {
    for crossing in crossings {
        if matches!(bridge, Some(EdgeEndpoint::From | EdgeEndpoint::Both))
            && crossing.distance_from_start <= ENDPOINT_TOUCH_METERS
            || matches!(bridge, Some(EdgeEndpoint::To | EdgeEndpoint::Both))
                && crossing.distance_to_end <= ENDPOINT_TOUCH_METERS
        {
            crossing.crossing.traversal = CrossingTraversal::Bridge;
        }
    }
}

fn add_unmapped_bridges(
    crossings: &mut Vec<LocatedCrossing>,
    bridge: Option<EdgeEndpoint>,
    route_length: f64,
) {
    let mut push = |position: u16, distance_from_start: f64, distance_to_end: f64| {
        let endpoint_is_mapped = crossings.iter().any(|crossing| {
            if position == 0 {
                crossing.distance_from_start <= ENDPOINT_TOUCH_METERS
            } else {
                crossing.distance_to_end <= ENDPOINT_TOUCH_METERS
            }
        });
        if endpoint_is_mapped {
            return;
        }
        crossings.push(LocatedCrossing {
            crossing: LandWaterCrossing {
                position: EdgeProgressPermille::new(position).expect("constant is valid"),
                watercourse: CrossingWatercourse::River(RiverWatercourse {
                    order: StrahlerOrder::new(2).expect("constant is valid"),
                    persistence: FlowPersistence::Perennial,
                }),
                traversal: CrossingTraversal::Bridge,
            },
            distance_from_start,
            distance_to_end,
        });
    };
    match bridge {
        Some(EdgeEndpoint::From) => push(0, 0.0, route_length),
        Some(EdgeEndpoint::To) => push(1_000, route_length, 0.0),
        Some(EdgeEndpoint::Both) => {
            push(0, 0.0, route_length);
            push(1_000, route_length, 0.0);
        }
        None => {}
    }
}

fn sort_and_deduplicate_crossings(crossings: &mut Vec<LocatedCrossing>) {
    crossings.sort_by(|left, right| {
        left.distance_from_start
            .total_cmp(&right.distance_from_start)
    });
    crossings.dedup_by(|left, right| {
        (left.distance_from_start - right.distance_from_start).abs() <= CROSSING_DEDUP_METERS
            && left.crossing.watercourse == right.crossing.watercourse
    });
}

fn collect_geopackages(directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_geopackages(&path, output)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gpkg"))
        {
            output.push(path);
        }
    }
    Ok(())
}

fn read_geopackage(
    path: &Path,
    bounds: Option<Bounds>,
    output: &mut Vec<WaterFeature>,
) -> Result<()> {
    let connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|source| {
            Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            }
        })?;
    let application_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    if application_id != 0x4750_4b47 {
        return Err(Error::Validation(format!(
            "{} is not an OGC GeoPackage",
            path.display()
        )));
    }
    let (organization, organization_srs): (String, i64) = connection
        .query_row(
            "SELECT organization, organization_coordsys_id FROM gpkg_spatial_ref_sys WHERE srs_id = ?1",
            params![EXPECTED_SRS],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|source| Error::GeoPackage { path: path.to_path_buf(), source })?;
    if !organization.eq_ignore_ascii_case("EPSG") || organization_srs != EXPECTED_SRS {
        return Err(Error::Validation(format!(
            "{} does not bind GeoPackage SRS {EXPECTED_SRS} to EPSG:{EXPECTED_SRS}",
            path.display()
        )));
    }
    let mut tables = connection
        .prepare(
            "SELECT table_name, column_name, geometry_type_name, srs_id FROM gpkg_geometry_columns",
        )
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    let metadata = tables
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    for (table, geometry_column, geometry_type, srs) in metadata {
        let Some(kind) = feature_kind(&table) else {
            continue;
        };
        let contents_srs: i64 = connection
            .query_row(
                "SELECT srs_id FROM gpkg_contents WHERE table_name = ?1 AND data_type = 'features'",
                params![table],
                |row| row.get(0),
            )
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        if srs != EXPECTED_SRS {
            return Err(Error::Validation(format!(
                "{} table {table} uses EPSG:{srs}; expected EPSG:{EXPECTED_SRS}",
                path.display()
            )));
        }
        if contents_srs != EXPECTED_SRS {
            return Err(Error::Validation(format!(
                "{} table {table} is registered in gpkg_contents with EPSG:{contents_srs}; expected EPSG:{EXPECTED_SRS}",
                path.display()
            )));
        }
        if !source_geometry_type_matches(kind, &geometry_type) {
            return Err(Error::Validation(format!(
                "{} table {table} has incompatible geometry type {geometry_type}",
                path.display()
            )));
        }
        read_feature_table(
            &connection,
            path,
            &table,
            &geometry_column,
            kind,
            bounds,
            output,
        )?;
    }
    Ok(())
}

fn feature_kind(table: &str) -> Option<FeatureKind> {
    match table.to_ascii_lowercase().as_str() {
        "river_net_l" | "river_net_lines" => Some(FeatureKind::River),
        "canals_l" => Some(FeatureKind::Canal),
        "ditches_l" => Some(FeatureKind::Ditch),
        "inlandwater" => Some(FeatureKind::InlandWater),
        "transit_p" | "transit_polygon" => Some(FeatureKind::Tidal),
        "coastal_p" | "coastal_polygon" => Some(FeatureKind::Coastal),
        _ => None,
    }
}

fn read_feature_table(
    connection: &Connection,
    path: &Path,
    table: &str,
    geometry_column: &str,
    kind: FeatureKind,
    bounds: Option<Bounds>,
    output: &mut Vec<WaterFeature>,
) -> Result<()> {
    let layout = table_layout(connection, path, table)?;
    let value = |name: &str| {
        if layout
            .columns
            .iter()
            .any(|column| column.eq_ignore_ascii_case(name))
        {
            format!(
                "f.{}",
                quote_identifier(
                    layout
                        .columns
                        .iter()
                        .find(|column| column.eq_ignore_ascii_case(name))
                        .unwrap(),
                )
            )
        } else {
            "NULL".into()
        }
    };
    let rtree = format!("rtree_{table}_{geometry_column}");
    let use_rtree = bounds.is_some()
        && layout.integer_primary_key.is_some()
        && table_exists(connection, path, &rtree)?
        && rtree_is_registered(connection, path, table, geometry_column)?;
    if use_rtree {
        require_complete_rtree(
            connection,
            path,
            table,
            geometry_column,
            layout.integer_primary_key.as_ref().unwrap(),
            &rtree,
        )?;
    }
    let from = if use_rtree {
        format!(
            "{} AS f JOIN {} AS r ON r.id = f.{}",
            quote_identifier(table),
            quote_identifier(&rtree),
            quote_identifier(layout.integer_primary_key.as_ref().unwrap())
        )
    } else {
        format!("{} AS f", quote_identifier(table))
    };
    let where_clause = if use_rtree {
        " WHERE r.maxx >= ?1 AND r.minx <= ?2 AND r.maxy >= ?3 AND r.miny <= ?4"
    } else {
        ""
    };
    let sql = format!(
        "SELECT f.{}, {}, {}, {}, {} FROM {from}{where_clause}",
        quote_identifier(geometry_column),
        value("STRAHLER"),
        value("HYP"),
        value("NVS"),
        value("AREA_GEO"),
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    let mut rows = if let Some(bounds) = bounds.filter(|_| use_rtree) {
        statement.query(params![
            bounds.min_x,
            bounds.max_x,
            bounds.min_y,
            bounds.max_y
        ])
    } else {
        statement.query([])
    }
    .map_err(|source| Error::GeoPackage {
        path: path.to_path_buf(),
        source,
    })?;
    while let Some(row) = rows.next().map_err(|source| Error::GeoPackage {
        path: path.to_path_buf(),
        source,
    })? {
        let geometry = row
            .get::<_, Vec<u8>>(0)
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let order = row
            .get::<_, Option<i64>>(1)
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let persistence = row
            .get::<_, Option<i64>>(2)
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let navigability = row
            .get::<_, Option<i64>>(3)
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let area = row
            .get::<_, Option<f64>>(4)
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let geometry = GpkgWkb(geometry).to_geo().map_err(|error| {
            Error::Validation(format!(
                "{} table {table} has invalid GeoPackage geometry: {error}",
                path.display()
            ))
        })?;
        if !decoded_geometry_matches(kind, &geometry) {
            return Err(Error::Validation(format!(
                "{} table {table} contains geometry incompatible with its feature class",
                path.display()
            )));
        }
        if bounds.is_some_and(|bounds| !geometry_overlaps_bounds(&geometry, bounds)) {
            continue;
        }
        if persistence == Some(4) {
            continue;
        }
        let area_square_meters = area
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or_else(|| geometry_area_hint(&geometry));
        output.push(WaterFeature {
            kind,
            geometry,
            order: StrahlerOrder::new(
                order
                    .and_then(|value| u8::try_from(value).ok())
                    .filter(|value| (1..=StrahlerOrder::MAX).contains(value))
                    .unwrap_or(1),
            )
            .expect("value is normalized"),
            persistence: match persistence {
                Some(2) => FlowPersistence::Intermittent,
                Some(3) => FlowPersistence::Ephemeral,
                _ => FlowPersistence::Perennial,
            },
            navigable: matches!(navigability, Some(1 | 3 | 4)),
            area_square_meters,
        });
    }
    Ok(())
}

fn source_geometry_type_matches(kind: FeatureKind, geometry_type: &str) -> bool {
    let geometry_type = geometry_type.to_ascii_uppercase();
    match kind {
        FeatureKind::River | FeatureKind::Canal | FeatureKind::Ditch => {
            matches!(geometry_type.as_str(), "LINESTRING" | "MULTILINESTRING")
        }
        FeatureKind::InlandWater | FeatureKind::Tidal | FeatureKind::Coastal => {
            matches!(geometry_type.as_str(), "POLYGON" | "MULTIPOLYGON")
        }
    }
}

fn decoded_geometry_matches(kind: FeatureKind, geometry: &Geometry<f64>) -> bool {
    match kind {
        FeatureKind::River | FeatureKind::Canal | FeatureKind::Ditch => {
            matches!(
                geometry,
                Geometry::LineString(_) | Geometry::MultiLineString(_)
            )
        }
        FeatureKind::InlandWater | FeatureKind::Tidal | FeatureKind::Coastal => {
            matches!(geometry, Geometry::Polygon(_) | Geometry::MultiPolygon(_))
        }
    }
}

struct TableLayout {
    columns: Vec<String>,
    integer_primary_key: Option<String>,
}

fn table_layout(connection: &Connection, path: &Path, table: &str) -> Result<TableLayout> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({})", quote_identifier(table)))
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    let integer_primary_key = columns
        .iter()
        .find(|(_, data_type, primary_key)| {
            *primary_key > 0 && data_type.to_ascii_uppercase().contains("INT")
        })
        .map(|(name, _, _)| name.clone());
    Ok(TableLayout {
        columns: columns.into_iter().map(|(name, _, _)| name).collect(),
        integer_primary_key,
    })
}

fn table_exists(connection: &Connection, path: &Path, table: &str) -> Result<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1)",
            params![table],
            |row| row.get(0),
        )
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })
}

fn rtree_is_registered(
    connection: &Connection,
    path: &Path,
    table: &str,
    geometry_column: &str,
) -> Result<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM gpkg_extensions WHERE table_name = ?1 AND column_name = ?2 AND extension_name = 'gpkg_rtree_index')",
            params![table, geometry_column],
            |row| row.get(0),
        )
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })
}

fn require_complete_rtree(
    connection: &Connection,
    path: &Path,
    table: &str,
    geometry_column: &str,
    primary_key: &str,
    rtree: &str,
) -> Result<()> {
    let sql = format!(
        "SELECT COUNT(*) FROM {} AS f LEFT JOIN {} AS r ON r.id = f.{} WHERE f.{} IS NOT NULL AND r.id IS NULL",
        quote_identifier(table),
        quote_identifier(rtree),
        quote_identifier(primary_key),
        quote_identifier(geometry_column),
    );
    let missing: i64 = connection
        .query_row(&sql, [], |row| row.get(0))
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    if missing == 0 {
        Ok(())
    } else {
        Err(Error::Validation(format!(
            "{} table {table} has {missing} geometries missing from its GeoPackage RTree",
            path.display()
        )))
    }
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn geometry_overlaps_bounds(geometry: &Geometry<f64>, bounds: Bounds) -> bool {
    geometry.bounding_rect().is_some_and(|rect| {
        rect.max().x >= bounds.min_x
            && rect.min().x <= bounds.max_x
            && rect.max().y >= bounds.min_y
            && rect.min().y <= bounds.max_y
    })
}

fn geometry_area_hint(geometry: &Geometry<f64>) -> f64 {
    geometry
        .bounding_rect()
        .map_or(0.0, |rect| rect.width() * rect.height())
}

fn geometry_contains(geometry: &Geometry<f64>, point: Point<f64>) -> bool {
    match geometry {
        Geometry::Polygon(polygon) => polygon.contains(&point),
        Geometry::MultiPolygon(polygons) => polygons.contains(&point),
        Geometry::GeometryCollection(collection) => {
            collection.iter().any(|item| geometry_contains(item, point))
        }
        _ => false,
    }
}

fn geometry_distance(point: Point<f64>, geometry: &Geometry<f64>) -> Option<f64> {
    if geometry_contains(geometry, point) {
        return Some(0.0);
    }
    match geometry {
        Geometry::Line(line) => Some(point_segment_distance(point.0, *line)),
        Geometry::LineString(line) => line_distance(point.0, line),
        Geometry::MultiLineString(lines) => lines
            .iter()
            .filter_map(|line| line_distance(point.0, line))
            .min_by(f64::total_cmp),
        Geometry::Polygon(polygon) => line_distance(point.0, polygon.exterior()),
        Geometry::MultiPolygon(polygons) => polygons
            .iter()
            .filter_map(|polygon| line_distance(point.0, polygon.exterior()))
            .min_by(f64::total_cmp),
        Geometry::Point(other) => {
            Some(((point.x() - other.x()).powi(2) + (point.y() - other.y()).powi(2)).sqrt())
        }
        Geometry::GeometryCollection(collection) => collection
            .iter()
            .filter_map(|item| geometry_distance(point, item))
            .min_by(f64::total_cmp),
        _ => None,
    }
}

fn line_distance(point: Coord<f64>, line: &LineString<f64>) -> Option<f64> {
    line.lines()
        .map(|segment| point_segment_distance(point, segment))
        .min_by(f64::total_cmp)
}

fn point_segment_distance(point: Coord<f64>, segment: Line<f64>) -> f64 {
    let dx = segment.end.x - segment.start.x;
    let dy = segment.end.y - segment.start.y;
    if dx == 0.0 && dy == 0.0 {
        return ((point.x - segment.start.x).powi(2) + (point.y - segment.start.y).powi(2)).sqrt();
    }
    let t = (((point.x - segment.start.x) * dx + (point.y - segment.start.y) * dy)
        / (dx * dx + dy * dy))
        .clamp(0.0, 1.0);
    ((point.x - (segment.start.x + t * dx)).powi(2)
        + (point.y - (segment.start.y + t * dy)).powi(2))
    .sqrt()
}

fn geometry_line_intersections(route: Line<f64>, geometry: &Geometry<f64>) -> Vec<f64> {
    let mut output = Vec::new();
    match geometry {
        Geometry::Line(line) => push_intersection(route, *line, &mut output),
        Geometry::LineString(line) => {
            for segment in line.lines() {
                push_intersection(route, segment, &mut output);
            }
        }
        Geometry::MultiLineString(lines) => {
            for line in lines {
                for segment in line.lines() {
                    push_intersection(route, segment, &mut output);
                }
            }
        }
        Geometry::GeometryCollection(collection) => {
            for item in collection {
                output.extend(geometry_line_intersections(route, item));
            }
        }
        _ => {}
    }
    output
}

fn push_intersection(route: Line<f64>, water: Line<f64>, output: &mut Vec<f64>) {
    use geo::line_intersection::{LineIntersection, line_intersection};
    if let Some(intersection) = line_intersection(route, water) {
        let point = match intersection {
            LineIntersection::SinglePoint { intersection, .. } => intersection,
            LineIntersection::Collinear { intersection } => intersection.start,
        };
        let dx = route.end.x - route.start.x;
        let dy = route.end.y - route.start.y;
        let denominator = dx * dx + dy * dy;
        if denominator > 0.0 {
            let progress =
                ((point.x - route.start.x) * dx + (point.y - route.start.y) * dy) / denominator;
            output.push(progress.clamp(0.0, 1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use rusqlite::params;

    use super::*;

    #[test]
    fn canal_access_cannot_exist_without_river_access() {
        let access = FlowingWaterAccess::RiverAndCanal(RiverAndCanalAccess {
            river: RiverAccess {
                distance: WaterDistanceMeters::new(100).unwrap(),
                order: StrahlerOrder::new(3).unwrap(),
                persistence: FlowPersistence::Perennial,
            },
            canal_distance: WaterDistanceMeters::new(50).unwrap(),
            canal_navigable: true,
        });
        assert!(matches!(access, FlowingWaterAccess::RiverAndCanal(_)));
    }

    #[test]
    fn line_intersection_records_progress() {
        let route = Line::new(Coord { x: 0.0, y: 0.0 }, Coord { x: 100.0, y: 0.0 });
        let water = Geometry::Line(Line::new(
            Coord { x: 25.0, y: -10.0 },
            Coord { x: 25.0, y: 10.0 },
        ));
        assert_eq!(geometry_line_intersections(route, &water), vec![0.25]);
    }

    #[test]
    fn bridge_evidence_supplies_missing_source_crossing() {
        let mut crossings = Vec::new();
        add_unmapped_bridges(&mut crossings, Some(EdgeEndpoint::Both), 100.0);
        assert_eq!(crossings.len(), 2);
        assert!(
            crossings
                .iter()
                .all(|crossing| { crossing.crossing.traversal == CrossingTraversal::Bridge })
        );
    }

    #[test]
    fn touching_water_at_road_endpoint_is_not_a_crossing_without_bridge_evidence() {
        let fixture = Fixture::new();
        let database = HydrologyDatabase::open(&fixture.directory, None).unwrap();
        let (route, count, _) = database
            .enrich_route(
                TravelRouteDraft::Land { bridge: None },
                Point::new(0.0, 0.0),
                Point::new(200.0, 0.0),
            )
            .unwrap();
        let TravelRoute::Land(route) = route else {
            panic!("expected land route")
        };
        assert_eq!(count, 1);
        assert_eq!(route.water_crossings[0].position.get(), 500);
        assert!(matches!(
            route.water_crossings[0].watercourse,
            CrossingWatercourse::Canal(_)
        ));
    }

    #[test]
    fn long_edges_keep_crossings_that_are_close_only_in_relative_terms() {
        let watercourse = CrossingWatercourse::River(RiverWatercourse {
            order: StrahlerOrder::new(3).unwrap(),
            persistence: FlowPersistence::Perennial,
        });
        let mut crossings = [400.0, 500.0]
            .into_iter()
            .map(|distance_from_start| LocatedCrossing {
                crossing: LandWaterCrossing {
                    position: EdgeProgressPermille::new(
                        (distance_from_start / 100_000.0 * 1_000.0) as u16,
                    )
                    .unwrap(),
                    watercourse: watercourse.clone(),
                    traversal: CrossingTraversal::Bridge,
                },
                distance_from_start,
                distance_to_end: 100_000.0 - distance_from_start,
            })
            .collect::<Vec<_>>();

        sort_and_deduplicate_crossings(&mut crossings);

        assert_eq!(crossings.len(), 2);
    }

    #[test]
    fn long_edge_endpoint_bridge_does_not_consume_a_distant_crossing() {
        let fixture = Fixture::new();
        let database = HydrologyDatabase::open(&fixture.directory, None).unwrap();
        let (route, count, _) = database
            .enrich_route(
                TravelRouteDraft::Land {
                    bridge: Some(EdgeEndpoint::From),
                },
                Point::new(-5_000.0, 0.0),
                Point::new(95_000.0, 0.0),
            )
            .unwrap();
        let TravelRoute::Land(route) = route else {
            panic!("expected land route")
        };
        assert_eq!(count, 3);
        assert_eq!(route.water_crossings[0].position.get(), 0);
        assert!(matches!(
            route.water_crossings[0].watercourse,
            CrossingWatercourse::River(RiverWatercourse {
                order,
                persistence: FlowPersistence::Perennial,
            }) if order.get() == 2
        ));
        assert!(matches!(
            route.water_crossings[1].watercourse,
            CrossingWatercourse::River(RiverWatercourse {
                order,
                persistence: FlowPersistence::Intermittent,
            }) if order.get() == 3
        ));
    }

    #[test]
    fn endpoint_bridge_evidence_preserves_mapped_watercourse_attributes() {
        let fixture = Fixture::new();
        let database = HydrologyDatabase::open(&fixture.directory, None).unwrap();
        let (route, count, _) = database
            .enrich_route(
                TravelRouteDraft::Land {
                    bridge: Some(EdgeEndpoint::From),
                },
                Point::new(0.0, 0.0),
                Point::new(200.0, 0.0),
            )
            .unwrap();
        let TravelRoute::Land(route) = route else {
            panic!("expected land route")
        };
        assert_eq!(count, 2);
        let river = &route.water_crossings[0];
        assert_eq!(river.position.get(), 0);
        assert_eq!(river.traversal, CrossingTraversal::Bridge);
        assert!(matches!(
            river.watercourse,
            CrossingWatercourse::River(RiverWatercourse {
                order,
                persistence: FlowPersistence::Intermittent,
            }) if order.get() == 3
        ));
    }

    #[test]
    fn geopackage_fixture_parses_adjacency_and_crossings() {
        let fixture = Fixture::new();
        let database = HydrologyDatabase::open(&fixture.directory, None).unwrap();
        assert_eq!(database.files_read, 1);
        assert_eq!(database.features.len(), 4);

        let hydrology = database.sample_settlement(Point::new(50.0, 0.0)).unwrap();
        let FlowingWaterAccess::RiverAndCanal(access) = hydrology.flowing.unwrap() else {
            panic!("expected combined river and canal access");
        };
        assert_eq!(access.river.order.get(), 3);
        assert_eq!(access.river.persistence, FlowPersistence::Intermittent);
        assert!(access.canal_navigable);
        assert!(hydrology.has_freshwater());
        assert!(!hydrology.has_saltwater());

        let (route, count, inferred) = database
            .enrich_route(
                TravelRouteDraft::Land { bridge: None },
                Point::new(-100.0, 0.0),
                Point::new(200.0, 0.0),
            )
            .unwrap();
        let TravelRoute::Land(route) = route else {
            panic!("expected land route")
        };
        assert_eq!(count, 2);
        assert_eq!(route.water_crossings.len(), 2);
        assert!(!inferred);
    }

    #[test]
    fn raw_unknown_sentinels_are_resolved_at_source_boundary() {
        let fixture = Fixture::new();
        let connection = Connection::open(&fixture.path).unwrap();
        connection
            .execute(
                "INSERT INTO River_Net_l VALUES (2, ?1, -9999, -9999, NULL, NULL)",
                params![line_geopackage_geometry(300.0)],
            )
            .unwrap();
        drop(connection);
        let database = HydrologyDatabase::open(&fixture.directory, None).unwrap();
        let feature = database
            .features
            .iter()
            .find(|feature| {
                geometry_distance(Point::new(300.0, 0.0), &feature.geometry) == Some(0.0)
            })
            .unwrap();
        assert_eq!(feature.order.get(), 1);
        assert_eq!(feature.persistence, FlowPersistence::Perennial);
    }

    #[test]
    fn geopackage_rtree_filters_before_geometry_decoding() {
        let fixture = Fixture::new();
        let connection = Connection::open(&fixture.path).unwrap();
        connection
            .execute(
                "INSERT INTO Coastal_p VALUES (2, X'00', NULL, NULL, NULL, NULL)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO rtree_Coastal_p_geom VALUES (2, 10000, 10001, 10000, 10001)",
                [],
            )
            .unwrap();
        drop(connection);
        let database = HydrologyDatabase::open(
            &fixture.directory,
            Some(Bounds {
                min_x: -200.0,
                min_y: -200.0,
                max_x: 200.0,
                max_y: 200.0,
            }),
        )
        .unwrap();
        assert_eq!(database.features.len(), 2);
        assert!(
            database
                .features
                .iter()
                .all(|feature| matches!(feature.kind, FeatureKind::River | FeatureKind::Canal))
        );
    }

    #[test]
    fn registered_rtree_must_cover_every_geometry() {
        let fixture = Fixture::new();
        let connection = Connection::open(&fixture.path).unwrap();
        connection
            .execute("DELETE FROM rtree_River_Net_l_geom", [])
            .unwrap();
        drop(connection);
        let error = HydrologyDatabase::open(
            &fixture.directory,
            Some(Bounds {
                min_x: -200.0,
                min_y: -200.0,
                max_x: 200.0,
                max_y: 200.0,
            }),
        )
        .err()
        .expect("incomplete registered RTree must be rejected");
        assert!(
            error
                .to_string()
                .contains("missing from its GeoPackage RTree")
        );
    }

    #[test]
    fn line_feature_class_rejects_point_geometry_metadata() {
        let fixture = Fixture::new();
        let connection = Connection::open(&fixture.path).unwrap();
        connection
            .execute(
                "UPDATE gpkg_geometry_columns SET geometry_type_name = 'POINT' WHERE table_name = 'River_Net_l'",
                [],
            )
            .unwrap();
        drop(connection);
        let error = HydrologyDatabase::open(&fixture.directory, None)
            .err()
            .expect("point river geometry must be rejected");
        assert!(
            error
                .to_string()
                .contains("incompatible geometry type POINT")
        );
    }

    #[test]
    #[ignore = "requires extracted official EU-Hydro basin GeoPackages in EU_HYDRO_DIR"]
    fn reads_downloaded_eu_hydro_distribution() {
        let directory = std::env::var_os("EU_HYDRO_DIR").expect("set EU_HYDRO_DIR");
        let database = HydrologyDatabase::open(Path::new(&directory), None).unwrap();
        assert!(database.files_read > 0);
        assert!(
            database
                .features
                .iter()
                .any(|feature| feature.kind == FeatureKind::River)
        );
        assert!(!database.spatial_grid.is_empty());
    }

    struct Fixture {
        directory: PathBuf,
        path: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = std::env::temp_dir().join(format!(
                "adventuresim-eu-hydro-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&directory).unwrap();
            let path = directory.join("fixture.gpkg");
            let connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "application_id", 0x4750_4b47_i64)
                .unwrap();
            connection
                .pragma_update(None, "user_version", 10_300_i64)
                .unwrap();
            connection.execute_batch(
                r#"CREATE TABLE gpkg_spatial_ref_sys (srs_name TEXT NOT NULL, srs_id INTEGER NOT NULL PRIMARY KEY, organization TEXT NOT NULL, organization_coordsys_id INTEGER NOT NULL, definition TEXT NOT NULL, description TEXT);
                 CREATE TABLE gpkg_contents (table_name TEXT NOT NULL PRIMARY KEY, data_type TEXT NOT NULL, identifier TEXT UNIQUE, description TEXT DEFAULT '', last_change TEXT NOT NULL DEFAULT '2026-01-01T00:00:00.000Z', min_x REAL, min_y REAL, max_x REAL, max_y REAL, srs_id INTEGER);
                 CREATE TABLE gpkg_geometry_columns (table_name TEXT NOT NULL, column_name TEXT NOT NULL, geometry_type_name TEXT NOT NULL, srs_id INTEGER NOT NULL, z INTEGER NOT NULL, m INTEGER NOT NULL, PRIMARY KEY (table_name, column_name));
                 CREATE TABLE gpkg_extensions (table_name TEXT, column_name TEXT, extension_name TEXT NOT NULL, definition TEXT NOT NULL, scope TEXT NOT NULL, UNIQUE (table_name, column_name, extension_name));
                 CREATE TABLE River_Net_l (fid INTEGER PRIMARY KEY, geom BLOB, STRAHLER INTEGER, HYP INTEGER, NVS INTEGER, AREA_GEO REAL);
                 CREATE TABLE Canals_l (fid INTEGER PRIMARY KEY, geom BLOB, STRAHLER INTEGER, HYP INTEGER, NVS INTEGER, AREA_GEO REAL);
                 CREATE TABLE InlandWater (fid INTEGER PRIMARY KEY, geom BLOB, STRAHLER INTEGER, HYP INTEGER, NVS INTEGER, AREA_GEO REAL);
                 CREATE TABLE Coastal_p (fid INTEGER PRIMARY KEY, geom BLOB, STRAHLER INTEGER, HYP INTEGER, NVS INTEGER, AREA_GEO REAL);
                 CREATE VIRTUAL TABLE rtree_River_Net_l_geom USING rtree(id, minx, maxx, miny, maxy);
                 CREATE VIRTUAL TABLE rtree_Canals_l_geom USING rtree(id, minx, maxx, miny, maxy);
                 CREATE VIRTUAL TABLE rtree_InlandWater_geom USING rtree(id, minx, maxx, miny, maxy);
                 CREATE VIRTUAL TABLE rtree_Coastal_p_geom USING rtree(id, minx, maxx, miny, maxy);
                 INSERT INTO gpkg_spatial_ref_sys VALUES ('Undefined Cartesian', -1, 'NONE', -1, 'undefined', 'undefined Cartesian coordinate reference system');
                 INSERT INTO gpkg_spatial_ref_sys VALUES ('Undefined Geographic', 0, 'NONE', 0, 'undefined', 'undefined geographic coordinate reference system');
                 INSERT INTO gpkg_spatial_ref_sys VALUES ('ETRS89 / LAEA Europe', 3035, 'EPSG', 3035, 'PROJCS["ETRS89 / LAEA Europe",GEOGCS["ETRS89",DATUM["European_Terrestrial_Reference_System_1989",SPHEROID["GRS 1980",6378137,298.257222101]],PRIMEM["Greenwich",0],UNIT["degree",0.0174532925199433]],PROJECTION["Lambert_Azimuthal_Equal_Area"],PARAMETER["latitude_of_center",52],PARAMETER["longitude_of_center",10],PARAMETER["false_easting",4321000],PARAMETER["false_northing",3210000],UNIT["metre",1],AXIS["Northing",NORTH],AXIS["Easting",EAST],AUTHORITY["EPSG","3035"]]', 'EPSG:3035 fixture');
                 INSERT INTO gpkg_contents (table_name, data_type, identifier, min_x, min_y, max_x, max_y, srs_id) VALUES ('River_Net_l', 'features', 'River_Net_l', -1, -1000, 1, 1000, 3035);
                 INSERT INTO gpkg_contents (table_name, data_type, identifier, min_x, min_y, max_x, max_y, srs_id) VALUES ('Canals_l', 'features', 'Canals_l', 99, -1000, 101, 1000, 3035);
                 INSERT INTO gpkg_contents (table_name, data_type, identifier, min_x, min_y, max_x, max_y, srs_id) VALUES ('InlandWater', 'features', 'InlandWater', 400, -100, 600, 100, 3035);
                 INSERT INTO gpkg_contents (table_name, data_type, identifier, min_x, min_y, max_x, max_y, srs_id) VALUES ('Coastal_p', 'features', 'Coastal_p', 4900, -100, 5100, 100, 3035);
                 INSERT INTO gpkg_geometry_columns VALUES ('River_Net_l', 'geom', 'LINESTRING', 3035, 0, 0);
                 INSERT INTO gpkg_geometry_columns VALUES ('Canals_l', 'geom', 'LINESTRING', 3035, 0, 0);
                 INSERT INTO gpkg_geometry_columns VALUES ('InlandWater', 'geom', 'POLYGON', 3035, 0, 0);
                 INSERT INTO gpkg_geometry_columns VALUES ('Coastal_p', 'geom', 'POLYGON', 3035, 0, 0);
                 INSERT INTO gpkg_extensions VALUES ('River_Net_l', 'geom', 'gpkg_rtree_index', 'http://www.geopackage.org/spec/#extension_rtree', 'write-only');
                 INSERT INTO gpkg_extensions VALUES ('Canals_l', 'geom', 'gpkg_rtree_index', 'http://www.geopackage.org/spec/#extension_rtree', 'write-only');
                 INSERT INTO gpkg_extensions VALUES ('InlandWater', 'geom', 'gpkg_rtree_index', 'http://www.geopackage.org/spec/#extension_rtree', 'write-only');
                 INSERT INTO gpkg_extensions VALUES ('Coastal_p', 'geom', 'gpkg_rtree_index', 'http://www.geopackage.org/spec/#extension_rtree', 'write-only');"#
            ).unwrap();
            connection
                .execute(
                    "INSERT INTO River_Net_l VALUES (1, ?1, 3, 2, 5, NULL)",
                    params![line_geopackage_geometry(0.0)],
                )
                .unwrap();
            connection
                .execute_batch(
                    "INSERT INTO rtree_River_Net_l_geom VALUES (1, 0, 0, -1000, 1000);
                     INSERT INTO rtree_Canals_l_geom VALUES (1, 100, 100, -1000, 1000);
                     INSERT INTO rtree_InlandWater_geom VALUES (1, 400, 600, -100, 100);
                     INSERT INTO rtree_Coastal_p_geom VALUES (1, 4900, 5100, -100, 100);",
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO Canals_l VALUES (1, ?1, 2, 1, 1, NULL)",
                    params![line_geopackage_geometry(100.0)],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO InlandWater VALUES (1, ?1, NULL, NULL, NULL, 2000000.0)",
                    params![square_geopackage_geometry(500.0, 0.0, 100.0)],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO Coastal_p VALUES (1, ?1, NULL, NULL, NULL, 2000000.0)",
                    params![square_geopackage_geometry(5000.0, 0.0, 100.0)],
                )
                .unwrap();
            drop(connection);
            Self { directory, path }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn line_geopackage_geometry(x: f64) -> Vec<u8> {
        let mut bytes = gpkg_header(x, x, -1000.0, 1000.0);
        bytes.push(1);
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        for (x, y) in [(x, -1000.0_f64), (x, 1000.0)] {
            bytes.extend_from_slice(&x.to_le_bytes());
            bytes.extend_from_slice(&y.to_le_bytes());
        }
        bytes
    }

    fn square_geopackage_geometry(x: f64, y: f64, radius: f64) -> Vec<u8> {
        let mut bytes = gpkg_header(x - radius, x + radius, y - radius, y + radius);
        bytes.push(1);
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&5_u32.to_le_bytes());
        for (x, y) in [
            (x - radius, y - radius),
            (x + radius, y - radius),
            (x + radius, y + radius),
            (x - radius, y + radius),
            (x - radius, y - radius),
        ] {
            bytes.extend_from_slice(&x.to_le_bytes());
            bytes.extend_from_slice(&y.to_le_bytes());
        }
        bytes
    }

    fn gpkg_header(min_x: f64, max_x: f64, min_y: f64, max_y: f64) -> Vec<u8> {
        let mut bytes = b"GP\0\x03".to_vec();
        bytes.extend_from_slice(&(EXPECTED_SRS as i32).to_le_bytes());
        for coordinate in [min_x, max_x, min_y, max_y] {
            bytes.extend_from_slice(&coordinate.to_le_bytes());
        }
        bytes
    }
}
