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
use rusqlite::{Connection, OpenFlags};

use crate::{
    Error, Result,
    draft::{DroughtSettlementDraft, TravelEdgeDraft, TravelRouteDraft, WorldDraft},
};

const SOURCE_NAME: &str = "Copernicus EU-Hydro River Network Database v1.3";
const SOURCE_URL: &str =
    "https://land.copernicus.eu/en/products/eu-hydro/eu-hydro-river-network-database";
const SOURCE_LICENSE: &str = "Copernicus Land Monitoring Service full and open data policy";
const EXPECTED_SRS: i64 = 3035;
const SETTLEMENT_ADJACENCY_METERS: f64 = 2_000.0;
const SOURCE_MARGIN_METERS: f64 = 10_000.0;
const CROSSING_DEDUP_PERMILLE: u16 = 5;
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

    let mut settlement_fallbacks = 0;
    let hydrology = draft
        .settlements
        .iter()
        .map(|settlement| {
            let base = base_settlement(settlement);
            let point = projected_nodes[&base.source_node_id];
            let value = database.sample_settlement(point)?;
            settlement_fallbacks += usize::from(value == SettlementHydrology::default());
            Ok(value)
        })
        .collect::<Result<Vec<_>>>()?;

    let mut crossings = 0;
    let mut inferred_ferries = 0;
    let edges = std::mem::take(&mut draft.edges)
        .into_iter()
        .map(|edge| {
            let from = projected_nodes[&edge.from_node_id];
            let to = projected_nodes[&edge.to_node_id];
            let (route, edge_crossings, inferred) = database.enrich_route(edge.route, from, to)?;
            crossings += edge_crossings;
            inferred_ferries += usize::from(inferred);
            Ok(finish_edge(edge, route))
        })
        .collect::<Result<Vec<_>>>()?;

    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(hydrology)
        .map(|(drought, hydrology)| finish_settlement(drought, hydrology))
        .collect::<Vec<_>>();

    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.hydrology_files_read = database.files_read;
    draft.report.hydrology_features_read = database.features.len();
    draft.report.hydrology_settlement_samples = settlements.len();
    draft.report.hydrology_settlement_fallback_samples = settlement_fallbacks;
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
    }
}

fn base_settlement(draft: &DroughtSettlementDraft) -> &crate::draft::SettlementDraft {
    &draft
        .religious
        .geologic
        .soil
        .trees
        .vegetated
        .forest
        .land
        .elevated
        .settlement
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
                let mut crossings = self.line_crossings(line);
                apply_bridge_evidence(&mut crossings, bridge);
                add_unmapped_bridges(&mut crossings, bridge);
                crossings.sort_by_key(|crossing| crossing.position);
                crossings.dedup_by(|left, right| {
                    left.position.get().abs_diff(right.position.get()) <= CROSSING_DEDUP_PERMILLE
                        && left.watercourse == right.watercourse
                });
                let count = crossings.len();
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
                    .map(|crossing| ferry_from_crossing(crossing.watercourse))
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

    fn line_crossings(&self, route: Line<f64>) -> Vec<LandWaterCrossing> {
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
                    .map(move |position| LandWaterCrossing {
                        position: EdgeProgressPermille::new(position)
                            .expect("intersection is clamped"),
                        watercourse: feature_crossing(feature),
                        traversal: plausible_traversal(feature),
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

fn apply_bridge_evidence(crossings: &mut [LandWaterCrossing], bridge: Option<EdgeEndpoint>) {
    for crossing in crossings {
        let position = crossing.position.get();
        if matches!(bridge, Some(EdgeEndpoint::From | EdgeEndpoint::Both)) && position <= 100
            || matches!(bridge, Some(EdgeEndpoint::To | EdgeEndpoint::Both)) && position >= 900
        {
            crossing.traversal = CrossingTraversal::Bridge;
        }
    }
}

fn add_unmapped_bridges(crossings: &mut Vec<LandWaterCrossing>, bridge: Option<EdgeEndpoint>) {
    let mut push = |position: u16| {
        let endpoint_is_mapped = crossings.iter().any(|crossing| {
            if position == 0 {
                crossing.position.get() <= 100
            } else {
                crossing.position.get() >= 900
            }
        });
        if endpoint_is_mapped {
            return;
        }
        crossings.push(LandWaterCrossing {
            position: EdgeProgressPermille::new(position).expect("constant is valid"),
            watercourse: CrossingWatercourse::River(RiverWatercourse {
                order: StrahlerOrder::new(2).expect("constant is valid"),
                persistence: FlowPersistence::Perennial,
            }),
            traversal: CrossingTraversal::Bridge,
        });
    };
    match bridge {
        Some(EdgeEndpoint::From) => push(0),
        Some(EdgeEndpoint::To) => push(1_000),
        Some(EdgeEndpoint::Both) => {
            push(0);
            push(1_000);
        }
        None => {}
    }
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
        if srs != EXPECTED_SRS {
            return Err(Error::Validation(format!(
                "{} table {table} uses EPSG:{srs}; expected EPSG:{EXPECTED_SRS}",
                path.display()
            )));
        }
        let polygon = matches!(
            kind,
            FeatureKind::InlandWater | FeatureKind::Tidal | FeatureKind::Coastal
        );
        if polygon != geometry_type.to_ascii_uppercase().contains("POLYGON") {
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
    let columns = table_columns(connection, path, table)?;
    let value = |name: &str| {
        if columns
            .iter()
            .any(|column| column.eq_ignore_ascii_case(name))
        {
            quote_identifier(
                columns
                    .iter()
                    .find(|column| column.eq_ignore_ascii_case(name))
                    .unwrap(),
            )
        } else {
            "NULL".into()
        }
    };
    let sql = format!(
        "SELECT {}, {}, {}, {}, {} FROM {}",
        quote_identifier(geometry_column),
        value("STRAHLER"),
        value("HYP"),
        value("NVS"),
        value("AREA_GEO"),
        quote_identifier(table)
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<f64>>(4)?,
            ))
        })
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    for row in rows {
        let (geometry, order, persistence, navigability, area) =
            row.map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let geometry = GpkgWkb(geometry).to_geo().map_err(|error| {
            Error::Validation(format!(
                "{} table {table} has invalid GeoPackage geometry: {error}",
                path.display()
            ))
        })?;
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

fn table_columns(connection: &Connection, path: &Path, table: &str) -> Result<Vec<String>> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({})", quote_identifier(table)))
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?;
    statement
        .query_map([], |row| row.get(1))
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source| Error::GeoPackage {
            path: path.to_path_buf(),
            source,
        })
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

fn geometry_line_intersections(route: Line<f64>, geometry: &Geometry<f64>) -> Vec<u16> {
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

fn push_intersection(route: Line<f64>, water: Line<f64>, output: &mut Vec<u16>) {
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
            output.push((progress.clamp(0.0, 1.0) * 1_000.0).round() as u16);
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
        assert_eq!(geometry_line_intersections(route, &water), vec![250]);
    }

    #[test]
    fn bridge_evidence_supplies_missing_source_crossing() {
        let mut crossings = Vec::new();
        add_unmapped_bridges(&mut crossings, Some(EdgeEndpoint::Both));
        assert_eq!(crossings.len(), 2);
        assert!(
            crossings
                .iter()
                .all(|crossing| crossing.traversal == CrossingTraversal::Bridge)
        );
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
            connection.execute_batch(
                "CREATE TABLE gpkg_geometry_columns (table_name TEXT, column_name TEXT, geometry_type_name TEXT, srs_id INTEGER, z INTEGER, m INTEGER);
                 CREATE TABLE River_Net_l (fid INTEGER PRIMARY KEY, geom BLOB, STRAHLER INTEGER, HYP INTEGER, NVS INTEGER, AREA_GEO REAL);
                 CREATE TABLE Canals_l (fid INTEGER PRIMARY KEY, geom BLOB, STRAHLER INTEGER, HYP INTEGER, NVS INTEGER, AREA_GEO REAL);
                 CREATE TABLE InlandWater (fid INTEGER PRIMARY KEY, geom BLOB, STRAHLER INTEGER, HYP INTEGER, NVS INTEGER, AREA_GEO REAL);
                 CREATE TABLE Coastal_p (fid INTEGER PRIMARY KEY, geom BLOB, STRAHLER INTEGER, HYP INTEGER, NVS INTEGER, AREA_GEO REAL);
                 INSERT INTO gpkg_geometry_columns VALUES ('River_Net_l', 'geom', 'LINESTRING', 3035, 0, 0);
                 INSERT INTO gpkg_geometry_columns VALUES ('Canals_l', 'geom', 'LINESTRING', 3035, 0, 0);
                 INSERT INTO gpkg_geometry_columns VALUES ('InlandWater', 'geom', 'POLYGON', 3035, 0, 0);
                 INSERT INTO gpkg_geometry_columns VALUES ('Coastal_p', 'geom', 'POLYGON', 3035, 0, 0);"
            ).unwrap();
            connection
                .execute(
                    "INSERT INTO River_Net_l VALUES (1, ?1, 3, 2, 5, NULL)",
                    params![line_geopackage_geometry(0.0)],
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
