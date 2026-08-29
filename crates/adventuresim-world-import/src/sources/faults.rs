//! Bounded HIKE European Fault Database ingestion.

use std::{collections::BTreeMap, path::Path};

use adventuresim_world_schema::{
    CompiledWorld, MAX_FAULT_GEOMETRY_POINTS, MAX_FAULT_LINE_POINTS, MappedFault, TerrainFeature,
    TravelGeometryPoint,
};
use geo::{Geometry, LineString};
use geozero::{ToGeo, wkb::GpkgWkb};
use proj4rs::{proj::Proj, transform::transform};
use rusqlite::{Connection, OpenFlags, params};

use crate::{Error, Result};

const EXPECTED_SRS: i64 = 3034;
const SOURCES: [FaultSource; 2] = [
    FaultSource {
        geometry: "FaultGeometriesGermany_Saxony_Anhalt",
        attributes: "FaultAttributesGermany_Saxony_Anhalt",
        country: "DE-ST",
    },
    FaultSource {
        geometry: "FaultGeometriesGermany",
        attributes: "FaultAttributesGermany",
        country: "DE",
    },
];

struct FaultSource {
    geometry: &'static str,
    attributes: &'static str,
    country: &'static str,
}

pub(crate) fn enrich(
    mut world: CompiledWorld,
    geopackage: &Path,
    bounds: [f64; 4],
) -> Result<CompiledWorld> {
    let (faults, features_read) = read(geopackage, bounds)?;
    let points = faults.iter().map(|feature| feature.geometry().len()).sum();
    world.terrain_features = faults;
    world.report.fault_features_read = features_read;
    world.report.fault_traces_imported = world.terrain_features.len();
    world.report.fault_geometry_points = points;
    if !world.terrain_features.is_empty() {
        world
            .metadata
            .sources
            .push(crate::manifest::faults(geopackage)?);
        world.metadata.sources.sort_by(|a, b| a.id.cmp(&b.id));
        world.metadata.manifest_digest = crate::manifest::digest(
            world.metadata.world_year,
            world.metadata.spatial_grid,
            &world.metadata.sources,
        )?;
    }
    Ok(world)
}

fn read(path: &Path, bounds: [f64; 4]) -> Result<(Vec<TerrainFeature>, usize)> {
    if !path.is_file() {
        return Err(Error::MissingSource(path.to_path_buf()));
    }
    if bounds.iter().any(|value| !value.is_finite())
        || bounds[0] >= bounds[2]
        || bounds[1] >= bounds[3]
    {
        return Err(Error::Validation("invalid fault import bounds".into()));
    }
    let connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|source| {
            Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            }
        })?;
    validate_geopackage(&connection, path)?;
    let projection = Projection::new()?;
    let projected = projection.projected_bounds(bounds)?;
    let mut unique = BTreeMap::<Vec<(i32, i32)>, TerrainFeature>::new();
    let mut features_read = 0;
    for source in SOURCES {
        validate_source_table(&connection, path, &source)?;
        let sql = format!(
            "SELECT g.fid, g.geom, g.ID, a.LOCAL_NAME, a.FAULT_TYPE, a.ACTIVE, a.CAPABLE \
             FROM {geometry} g \
             JOIN rtree_{geometry}_geom r ON r.id = g.fid \
             LEFT JOIN {attributes} a ON a.ID = g.ID \
             WHERE r.maxx >= ?1 AND r.minx <= ?2 AND r.maxy >= ?3 AND r.miny <= ?4 \
             ORDER BY g.fid",
            geometry = source.geometry,
            attributes = source.attributes,
        );
        let mut statement = connection.prepare(&sql).map_err(|error| {
            Error::Validation(format!(
                "cannot query HIKE source table {}: {error}",
                source.geometry
            ))
        })?;
        let mut rows = statement
            .query(params![
                projected[0],
                projected[2],
                projected[1],
                projected[3]
            ])
            .map_err(|error| {
                Error::Validation(format!("cannot query HIKE fault bounds: {error}"))
            })?;
        while let Some(row) = rows
            .next()
            .map_err(|error| Error::Validation(format!("cannot read HIKE fault row: {error}")))?
        {
            features_read += 1;
            let fid: i64 = row.get(0).map_err(sql_error)?;
            let blob: Vec<u8> = row.get(1).map_err(sql_error)?;
            let source_id: Option<String> = row.get(2).map_err(sql_error)?;
            let local_name: Option<String> = row.get(3).map_err(sql_error)?;
            let fault_type: Option<String> = row.get(4).map_err(sql_error)?;
            let active: Option<String> = row.get(5).map_err(sql_error)?;
            let capable: Option<String> = row.get(6).map_err(sql_error)?;
            let geometry: Geometry<f64> = GpkgWkb(&blob).to_geo().map_err(|error| {
                Error::Validation(format!(
                    "invalid HIKE geometry {}:{fid}: {error}",
                    source.country
                ))
            })?;
            for (part, line) in line_strings(&geometry).into_iter().enumerate() {
                for (fragment, clipped) in clip_line(line, projected).into_iter().enumerate() {
                    let mut points = Vec::with_capacity(clipped.0.len());
                    for coordinate in clipped.0 {
                        let (longitude, latitude) =
                            projection.unproject(coordinate.x, coordinate.y)?;
                        let point = TravelGeometryPoint::new(
                            longitude.clamp(bounds[0], bounds[2]),
                            latitude.clamp(bounds[1], bounds[3]),
                        )
                        .map_err(Error::Validation)?;
                        if points.last() != Some(&point) {
                            points.push(point);
                        }
                    }
                    if points.len() < 2 {
                        continue;
                    }
                    simplify_to_bound(&mut points);
                    let key = points
                        .iter()
                        .map(|point| (point.longitude_e7, point.latitude_e7))
                        .collect::<Vec<_>>();
                    unique.entry(key).or_insert_with(|| {
                        TerrainFeature::MappedFault(MappedFault {
                            id: format!(
                                "{}:{}:{part}:{fragment}",
                                source.country,
                                source_id.as_deref().unwrap_or("unnamed")
                            ),
                            local_name: clean_text(local_name.as_deref()),
                            classification: clean_text(fault_type.as_deref()),
                            mapped_active: yes(active.as_deref()),
                            mapped_capable: yes(capable.as_deref()),
                            trace: points,
                        })
                    });
                }
            }
        }
    }
    let mut lines = unique.into_values().collect::<Vec<_>>();
    lines.sort_by(|a, b| a.id().cmp(b.id()));
    let point_count = lines
        .iter()
        .map(|feature| feature.geometry().len())
        .sum::<usize>();
    if point_count > MAX_FAULT_GEOMETRY_POINTS {
        return Err(Error::Validation(format!(
            "fault geometry has {point_count} points; maximum is {MAX_FAULT_GEOMETRY_POINTS}"
        )));
    }
    Ok((lines, features_read))
}

fn sql_error(error: rusqlite::Error) -> Error {
    Error::Validation(format!("invalid HIKE fault row: {error}"))
}

fn validate_geopackage(connection: &Connection, path: &Path) -> Result<()> {
    let application_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|source| Error::GeoPackage {
            path: path.into(),
            source,
        })?;
    if application_id != 0x4750_4b47 {
        return Err(Error::Validation(format!(
            "{} is not an OGC GeoPackage",
            path.display()
        )));
    }
    Ok(())
}

fn validate_source_table(connection: &Connection, path: &Path, source: &FaultSource) -> Result<()> {
    let (kind, srs): (String, i64) = connection
        .query_row(
            "SELECT geometry_type_name, srs_id FROM gpkg_geometry_columns WHERE table_name = ?1 AND column_name = 'geom'",
            [source.geometry],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| Error::Validation(format!("{} lacks required HIKE table {}: {error}", path.display(), source.geometry)))?;
    if kind != "MULTILINESTRING" || srs != EXPECTED_SRS {
        return Err(Error::Validation(format!(
            "HIKE table {} is {kind} EPSG:{srs}; expected MULTILINESTRING EPSG:{EXPECTED_SRS}",
            source.geometry
        )));
    }
    Ok(())
}

fn line_strings(geometry: &Geometry<f64>) -> Vec<&LineString<f64>> {
    match geometry {
        Geometry::LineString(line) => vec![line],
        Geometry::MultiLineString(lines) => lines.0.iter().collect(),
        _ => Vec::new(),
    }
}

fn clip_line(line: &LineString<f64>, bounds: [f64; 4]) -> Vec<LineString<f64>> {
    let mut result = Vec::new();
    let mut current = Vec::new();
    for pair in line.0.windows(2) {
        if let Some((a, b)) = clip_segment(pair[0], pair[1], bounds) {
            if current.last().copied() != Some(a) {
                if current.len() >= 2 {
                    result.push(LineString(std::mem::take(&mut current)));
                }
                current.push(a);
            }
            current.push(b);
        } else if current.len() >= 2 {
            result.push(LineString(std::mem::take(&mut current)));
        }
    }
    if current.len() >= 2 {
        result.push(LineString(current));
    }
    result
}

fn clip_segment(
    a: geo::Coord<f64>,
    b: geo::Coord<f64>,
    bounds: [f64; 4],
) -> Option<(geo::Coord<f64>, geo::Coord<f64>)> {
    let delta = b - a;
    let mut enter: f64 = 0.0;
    let mut leave: f64 = 1.0;
    for (p, q) in [
        (-delta.x, a.x - bounds[0]),
        (delta.x, bounds[2] - a.x),
        (-delta.y, a.y - bounds[1]),
        (delta.y, bounds[3] - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let ratio = q / p;
            if p < 0.0 {
                enter = enter.max(ratio);
            } else {
                leave = leave.min(ratio);
            }
        }
    }
    (enter <= leave).then(|| (a + delta * enter, a + delta * leave))
}

fn simplify_to_bound(points: &mut Vec<TravelGeometryPoint>) {
    if points.len() <= MAX_FAULT_LINE_POINTS {
        return;
    }
    let last = points.len() - 1;
    *points = (0..MAX_FAULT_LINE_POINTS)
        .map(|index| points[index * last / (MAX_FAULT_LINE_POINTS - 1)])
        .collect();
}

fn clean_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn yes(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("yes"))
}

struct Projection {
    geographic: Proj,
    projected: Proj,
}

impl Projection {
    fn new() -> Result<Self> {
        Ok(Self {
            geographic: Proj::from_proj_string(
                "+proj=longlat +datum=WGS84 +ellps=WGS84 +no_defs +type=crs",
            )?,
            projected: Proj::from_proj_string(
                "+proj=lcc +lat_0=52 +lon_0=10 +lat_1=35 +lat_2=65 +x_0=4000000 +y_0=2800000 +ellps=GRS80 +units=m +no_defs +type=crs",
            )?,
        })
    }

    fn project(&self, longitude: f64, latitude: f64) -> Result<(f64, f64)> {
        let mut coordinate = (longitude.to_radians(), latitude.to_radians(), 0.0);
        transform(&self.geographic, &self.projected, &mut coordinate)?;
        Ok((coordinate.0, coordinate.1))
    }

    fn unproject(&self, easting: f64, northing: f64) -> Result<(f64, f64)> {
        let mut coordinate = (easting, northing, 0.0);
        transform(&self.projected, &self.geographic, &mut coordinate)?;
        Ok((coordinate.0.to_degrees(), coordinate.1.to_degrees()))
    }

    fn projected_bounds(&self, bounds: [f64; 4]) -> Result<[f64; 4]> {
        let corners = [
            self.project(bounds[0], bounds[1])?,
            self.project(bounds[0], bounds[3])?,
            self.project(bounds[2], bounds[1])?,
            self.project(bounds[2], bounds[3])?,
        ];
        Ok(corners.into_iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |mut envelope, (x, y)| {
                envelope[0] = envelope[0].min(x);
                envelope[1] = envelope[1].min(y);
                envelope[2] = envelope[2].max(x);
                envelope[3] = envelope[3].max(y);
                envelope
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_clipping_keeps_only_bounded_fragments() {
        let line = LineString::from(vec![(-2.0, 0.5), (0.5, 0.5), (2.0, 0.5)]);
        let clipped = clip_line(&line, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(clipped.len(), 1);
        assert_eq!(
            clipped[0],
            LineString::from(vec![(0.0, 0.5), (0.5, 0.5), (1.0, 0.5)])
        );
    }

    #[test]
    fn projection_round_trip_is_stable_in_the_playable_area() {
        let projection = Projection::new().unwrap();
        let projected = projection.project(10.0, 51.5).unwrap();
        let geographic = projection.unproject(projected.0, projected.1).unwrap();
        assert!((geographic.0 - 10.0).abs() < 1e-7);
        assert!((geographic.1 - 51.5).abs() < 1e-7);
    }

    #[test]
    #[ignore = "requires the pinned 112 MB HIKE GeoPackage"]
    fn imports_pinned_hike_playable_subset() {
        let path = std::env::var_os("HIKE_TEST_GPKG").expect("HIKE_TEST_GPKG is required");
        let (lines, features_read) =
            read(Path::new(&path), adventuresim_world_schema::PLAYABLE_BOUNDS).unwrap();
        assert!(features_read > 100);
        assert!(!lines.is_empty());
        assert!(
            lines
                .iter()
                .any(|feature| matches!(feature, TerrainFeature::MappedFault(fault) if fault.local_name.as_deref() == Some("Leinetal-Graben")))
        );
        assert!(
            lines
                .iter()
                .flat_map(TerrainFeature::geometry)
                .all(|point| {
                    let [west, south, east, north] = adventuresim_world_schema::PLAYABLE_BOUNDS;
                    (west..=east).contains(&point.longitude())
                        && (south..=north).contains(&point.latitude())
                })
        );
    }
}
