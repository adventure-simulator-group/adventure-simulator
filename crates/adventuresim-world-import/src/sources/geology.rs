//! EGDI 1:1 million pan-European surface-geology sampling.

use std::path::{Path, PathBuf};

use adventuresim_world_schema::{
    CompiledWorld, GeologicAgeEvidence, GeologicEra, GeologicLithologyEvidence, GeologicSetting,
    GeologicUnitId, IgneousRock, MappedSurfaceGeology, MetamorphicRock, MixedLithology,
    SedimentaryRock, SettlementImport, SoilProfile, SoilSubstrate, SourceProvenance,
    SurfaceGeology, SurfaceLithology, UnconsolidatedDeposit, WORLD_SCHEMA_VERSION, WorldMetadata,
};
use geo::{Contains, Geometry, Point};
use geozero::{ToGeo, wkb::GpkgWkb};
use proj4rs::{proj::Proj, transform::transform};
use rusqlite::{Connection, OpenFlags, params};

use crate::{
    Error, Result,
    draft::{SoilSettlementDraft, WorldDraft},
};

const SOURCE_NAME: &str = "EGDI 1:1 Million pan-European Surface Geology";
const SOURCE_URL: &str =
    "https://metadata.europe-geology.eu/record/full/5729ffdf-2558-48fc-a5d2-645a0a010855";
const SOURCE_LICENSE: &str = "Creative Commons Attribution 4.0 (CC BY 4.0)";
const TABLE: &str = "GeologicUnitView";
const GEOMETRY_COLUMN: &str = "geom";
const EXPECTED_SRS: i64 = 3034;

pub(crate) fn enrich(
    draft: WorldDraft<SoilSettlementDraft>,
    geopackage: &Path,
) -> Result<CompiledWorld> {
    if draft.settlements.is_empty() {
        return finish(draft, Vec::new(), 0, 0);
    }
    let map = GeologyMap::open(geopackage)?;
    let projection = GeologyProjection::new()?;
    let mut profiles = Vec::with_capacity(draft.settlements.len());
    let mut fallbacks = 0;
    for settlement in &draft.settlements {
        let base = &settlement.trees.vegetated.forest.land.elevated.settlement;
        let point = projection.project(base.latitude, base.longitude)?;
        let profile = map.sample(point)?.unwrap_or_else(|| {
            fallbacks += 1;
            SurfaceGeology::Inferred(infer_setting(settlement))
        });
        profiles.push(profile);
    }
    finish(draft, profiles, map.features_read, fallbacks)
}

fn finish(
    mut draft: WorldDraft<SoilSettlementDraft>,
    profiles: Vec<SurfaceGeology>,
    features_read: usize,
    fallbacks: usize,
) -> Result<CompiledWorld> {
    if profiles.len() != draft.settlements.len() {
        return Err(Error::Validation(
            "geology profiles do not match settlements".into(),
        ));
    }
    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(profiles)
        .map(|(soil, geology)| {
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
                geology,
                scene_key: settlement.scene_key,
                religion_id: settlement.religion_id,
            }
        })
        .collect();
    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.geology_features_read = features_read;
    draft.report.geology_samples = draft.report.settlements;
    draft.report.geology_fallback_samples = fallbacks;
    Ok(CompiledWorld {
        metadata: WorldMetadata {
            schema_version: WORLD_SCHEMA_VERSION,
            world_year: draft.year,
            sources: draft.sources,
            road_types: draft.road_types,
        },
        nodes: draft.nodes,
        edges: draft.edges,
        settlements,
        report: draft.report,
    })
}

struct GeologyProjection {
    geographic: Proj,
    projected: Proj,
}

impl GeologyProjection {
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

    fn project(&self, latitude: f64, longitude: f64) -> Result<Point<f64>> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !(-180.0..=180.0).contains(&longitude)
        {
            return Err(Error::Validation(format!(
                "invalid coordinate ({latitude}, {longitude}) for EGDI"
            )));
        }
        let mut coordinate = (longitude.to_radians(), latitude.to_radians(), 0.0);
        transform(&self.geographic, &self.projected, &mut coordinate)?;
        Ok(Point::new(coordinate.0, coordinate.1))
    }
}

struct GeologyMap {
    connection: Connection,
    path: PathBuf,
    features_read: usize,
}

impl GeologyMap {
    fn open(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(Error::MissingSource(path.to_path_buf()));
        }
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
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
        let (geometry_type, srs): (String, i64) = connection
            .query_row(
                "SELECT geometry_type_name, srs_id FROM gpkg_geometry_columns WHERE table_name = ?1 AND column_name = ?2",
                params![TABLE, GEOMETRY_COLUMN],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        if !matches!(
            geometry_type.as_str(),
            "MULTISURFACE" | "MULTIPOLYGON" | "POLYGON"
        ) || srs != EXPECTED_SRS
        {
            return Err(Error::Validation(format!(
                "{} has EGDI geometry type {geometry_type:?} in EPSG:{srs}; expected polygonal EPSG:{EXPECTED_SRS}",
                path.display()
            )));
        }
        let features: i64 = connection
            .query_row("SELECT COUNT(*) FROM GeologicUnitView", [], |row| {
                row.get(0)
            })
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let features_read = usize::try_from(features).map_err(|_| {
            Error::Validation(format!("{} has an invalid feature count", path.display()))
        })?;
        if features_read == 0 {
            return Err(Error::Validation(format!(
                "{} contains no EGDI geology features",
                path.display()
            )));
        }
        Ok(Self {
            connection,
            path: path.to_path_buf(),
            features_read,
        })
    }

    fn sample(&self, point: Point<f64>) -> Result<Option<SurfaceGeology>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT g.fid, g.geom, g.id, g.geolunitid, g.name, g.representativeage_uri, g.representativeage_title, g.representativelithology_uri, g.representativelithology_title
                 FROM GeologicUnitView g
                 JOIN rtree_GeologicUnitView_geom r ON r.id = g.fid
                 WHERE r.minx <= ?1 AND r.maxx >= ?1 AND r.miny <= ?2 AND r.maxy >= ?2
                 ORDER BY g.fid",
            )
            .map_err(|source| Error::GeoPackage {
                path: self.path.clone(),
                source,
            })?;
        let rows = statement
            .query_map(params![point.x(), point.y()], |row| {
                Ok(RawGeologyFeature {
                    fid: row.get(0)?,
                    geometry: row.get(1)?,
                    id: row.get(2)?,
                    geologic_unit_id: row.get(3)?,
                    name: row.get(4)?,
                    age_uri: row.get(5)?,
                    age_title: row.get(6)?,
                    lithology_uri: row.get(7)?,
                    lithology_title: row.get(8)?,
                })
            })
            .map_err(|source| Error::GeoPackage {
                path: self.path.clone(),
                source,
            })?;
        for row in rows {
            let feature = row.map_err(|source| Error::GeoPackage {
                path: self.path.clone(),
                source,
            })?;
            let geometry =
                GpkgWkb(&feature.geometry)
                    .to_geo()
                    .map_err(|source| Error::InvalidField {
                        path: self.path.clone(),
                        field: "geom",
                        value: feature.fid.to_string(),
                        message: source.to_string(),
                    })?;
            if geometry_contains(&geometry, &point) {
                return Ok(Some(feature.into_profile(&self.path)?));
            }
        }
        Ok(None)
    }
}

fn geometry_contains(geometry: &Geometry<f64>, point: &Point<f64>) -> bool {
    match geometry {
        Geometry::Polygon(polygon) => polygon.contains(point),
        Geometry::MultiPolygon(polygons) => polygons.contains(point),
        Geometry::GeometryCollection(collection) => collection
            .iter()
            .any(|geometry| geometry_contains(geometry, point)),
        _ => false,
    }
}

struct RawGeologyFeature {
    fid: i64,
    geometry: Vec<u8>,
    id: Option<String>,
    geologic_unit_id: Option<String>,
    name: Option<String>,
    age_uri: Option<String>,
    age_title: Option<String>,
    lithology_uri: Option<String>,
    lithology_title: Option<String>,
}

impl RawGeologyFeature {
    fn into_profile(self, path: &Path) -> Result<SurfaceGeology> {
        let unit_value = [self.id, self.geologic_unit_id, self.name]
            .into_iter()
            .flatten()
            .map(|value| value.trim().to_owned())
            .find(|value| !is_missing(value))
            .unwrap_or_else(|| format!("egdi-fid-{}", self.fid));
        let unit = GeologicUnitId::new(unit_value.clone()).ok_or_else(|| Error::InvalidField {
            path: path.to_path_buf(),
            field: "id",
            value: unit_value,
            message: "identifier must be 1..=255 trimmed characters without controls".into(),
        })?;
        let lithology_label = preferred_label(self.lithology_title, self.lithology_uri);
        let lithology = if is_missing(&lithology_label) {
            GeologicLithologyEvidence::Inferred(SurfaceLithology::Sedimentary(
                SedimentaryRock::Sandstone,
            ))
        } else {
            GeologicLithologyEvidence::Mapped(classify_lithology(&lithology_label))
        };
        let lithology_value = match lithology {
            GeologicLithologyEvidence::Mapped(value)
            | GeologicLithologyEvidence::Inferred(value) => value,
        };
        let age_label = preferred_label(self.age_title, self.age_uri);
        let age = classify_age(&age_label)
            .map(GeologicAgeEvidence::Mapped)
            .unwrap_or_else(|| GeologicAgeEvidence::Inferred(infer_age(lithology_value)));
        Ok(SurfaceGeology::Mapped(MappedSurfaceGeology {
            unit,
            setting: GeologicSetting { lithology, age },
        }))
    }
}

fn preferred_label(title: Option<String>, uri: Option<String>) -> String {
    title
        .into_iter()
        .chain(uri)
        .map(|value| value.trim().to_owned())
        .find(|value| !is_missing(value))
        .unwrap_or_default()
}

fn is_missing(value: &str) -> bool {
    value.is_empty()
        || matches!(
            value.to_ascii_lowercase().as_str(),
            "unknown" | "nil" | "missing" | "notavailable" | "not available"
        )
        || value.contains("/nil/OGC/")
}

fn classify_lithology(value: &str) -> SurfaceLithology {
    use IgneousRock as I;
    use MetamorphicRock as M;
    use SedimentaryRock as S;
    use SurfaceLithology as L;
    use UnconsolidatedDeposit as U;
    let value = value.to_ascii_lowercase();
    let has = |needle: &str| value.contains(needle);
    if has("limestone") || has("carbonate sedimentary") {
        L::Sedimentary(S::Limestone)
    } else if has("dolom") {
        L::Sedimentary(S::Dolostone)
    } else if has("chalk") {
        L::Sedimentary(S::Chalk)
    } else if has("marl") {
        L::Sedimentary(S::Marl)
    } else if has("sandstone") || has("arenite") {
        L::Sedimentary(S::Sandstone)
    } else if has("siltstone") {
        L::Sedimentary(S::Siltstone)
    } else if has("mudstone") {
        L::Sedimentary(S::Mudstone)
    } else if has("shale") {
        L::Sedimentary(S::Shale)
    } else if has("conglomerate") {
        L::Sedimentary(S::Conglomerate)
    } else if has("evaporite") || has("gypsum") || has("halite") {
        L::Sedimentary(S::Evaporite)
    } else if has("coal") {
        L::Sedimentary(S::Coal)
    } else if has("chert") {
        L::Sedimentary(S::Chert)
    } else if has("granite") {
        L::Igneous(I::Granite)
    } else if has("granitoid") {
        L::Igneous(I::Granitoid)
    } else if has("diorite") {
        L::Igneous(I::Diorite)
    } else if has("gabbro") {
        L::Igneous(I::Gabbro)
    } else if has("basalt") {
        L::Igneous(I::Basalt)
    } else if has("andesite") {
        L::Igneous(I::Andesite)
    } else if has("rhyolite") {
        L::Igneous(I::Rhyolite)
    } else if has("tuff") {
        L::Igneous(I::Tuff)
    } else if has("plutonic") || has("intrusive") {
        L::Igneous(I::OtherPlutonic)
    } else if has("volcanic") || has("extrusive") || has("lava") {
        L::Igneous(I::OtherVolcanic)
    } else if has("slate") {
        L::Metamorphic(M::Slate)
    } else if has("schist") {
        L::Metamorphic(M::Schist)
    } else if has("gneiss") {
        L::Metamorphic(M::Gneiss)
    } else if has("quartzite") {
        L::Metamorphic(M::Quartzite)
    } else if has("marble") {
        L::Metamorphic(M::Marble)
    } else if has("phyllite") {
        L::Metamorphic(M::Phyllite)
    } else if has("amphibolite") {
        L::Metamorphic(M::Amphibolite)
    } else if has("metamorph") {
        L::Metamorphic(M::OtherMetamorphic)
    } else if has("clay") {
        L::Unconsolidated(U::Clay)
    } else if has("silt") {
        L::Unconsolidated(U::Silt)
    } else if has("sand") {
        L::Unconsolidated(U::Sand)
    } else if has("gravel") {
        L::Unconsolidated(U::Gravel)
    } else if has("diamicton") || has("till") {
        L::Unconsolidated(U::Till)
    } else if has("peat") || has("organic") {
        L::Unconsolidated(U::Peat)
    } else if has("alluv") || has("fluvial") {
        L::Unconsolidated(U::Alluvium)
    } else if has("loess") {
        L::Unconsolidated(U::Loess)
    } else if has("ash") || has("tephra") {
        L::Unconsolidated(U::VolcanicAsh)
    } else if has("sediment") || has("unconsolidated") {
        L::Unconsolidated(U::MixedSediment)
    } else if has("breccia") {
        L::Mixed(MixedLithology::Breccia)
    } else if has("melange") || has("mélange") {
        L::Mixed(MixedLithology::Melange)
    } else if has("sedimentary") {
        L::Sedimentary(S::MixedSedimentary)
    } else {
        L::Mixed(MixedLithology::MixedRock)
    }
}

fn classify_age(value: &str) -> Option<GeologicEra> {
    use GeologicEra as E;
    let value = value.to_ascii_lowercase();
    let has = |needle: &str| value.contains(needle);
    if is_missing(&value) {
        None
    } else if has("quaternary") || has("holocene") || has("pleistocene") {
        Some(E::Quaternary)
    } else if has("neogene") || has("miocene") || has("pliocene") {
        Some(E::Neogene)
    } else if has("paleogene")
        || has("palaeogene")
        || has("eocene")
        || has("oligocene")
        || has("paleocene")
    {
        Some(E::Paleogene)
    } else if has("cretaceous") {
        Some(E::Cretaceous)
    } else if has("jurassic") {
        Some(E::Jurassic)
    } else if has("triassic") {
        Some(E::Triassic)
    } else if has("permian") {
        Some(E::Permian)
    } else if has("carboniferous") || has("mississippian") || has("pennsylvanian") {
        Some(E::Carboniferous)
    } else if has("devonian") {
        Some(E::Devonian)
    } else if has("silurian") {
        Some(E::Silurian)
    } else if has("ordovician") {
        Some(E::Ordovician)
    } else if has("cambrian") {
        Some(E::Cambrian)
    } else if has("precambrian")
        || has("proterozoic")
        || has("archaean")
        || has("archean")
        || has("hadaean")
    {
        Some(E::Precambrian)
    } else {
        None
    }
}

const fn infer_age(lithology: SurfaceLithology) -> GeologicEra {
    match lithology {
        SurfaceLithology::Unconsolidated(_) => GeologicEra::Quaternary,
        SurfaceLithology::Sedimentary(SedimentaryRock::Coal) => GeologicEra::Carboniferous,
        SurfaceLithology::Sedimentary(_) => GeologicEra::Jurassic,
        SurfaceLithology::Igneous(_) => GeologicEra::Precambrian,
        SurfaceLithology::Metamorphic(_) => GeologicEra::Precambrian,
        SurfaceLithology::Mixed(_) => GeologicEra::Paleogene,
    }
}

fn infer_setting(settlement: &SoilSettlementDraft) -> GeologicSetting {
    let substrate = match &settlement.soil {
        SoilProfile::Mapped(profile) => profile.properties.substrate,
        SoilProfile::Inferred(properties) => properties.substrate,
    };
    let lithology = match substrate {
        SoilSubstrate::Organic(_) => SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Peat),
        SoilSubstrate::RockOutcrop(_) => SurfaceLithology::Metamorphic(MetamorphicRock::Schist),
        SoilSubstrate::Mineral(soil)
            if soil.depth == adventuresim_world_schema::SoilDepth::Shallow =>
        {
            SurfaceLithology::Sedimentary(SedimentaryRock::Limestone)
        }
        SoilSubstrate::Mineral(_) | SoilSubstrate::OtherNonTextured(_) => {
            SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone)
        }
    };
    GeologicSetting {
        lithology: GeologicLithologyEvidence::Inferred(lithology),
        age: GeologicAgeEvidence::Inferred(infer_age(lithology)),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::SystemTime};

    use geo::Point;
    use rusqlite::{Connection, params};

    use super::{
        GeologicAgeEvidence, GeologicEra, GeologicLithologyEvidence, GeologyMap, GeologyProjection,
        IgneousRock, SedimentaryRock, SurfaceGeology, SurfaceLithology, UnconsolidatedDeposit,
        classify_age, classify_lithology, infer_age,
    };

    #[test]
    fn classifies_specific_rocks_before_generic_sediment_words() {
        assert_eq!(
            classify_lithology("sandstone"),
            SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone)
        );
        assert_eq!(
            classify_lithology("volcaniclastic basalt"),
            SurfaceLithology::Igneous(IgneousRock::Basalt)
        );
        assert_eq!(
            classify_lithology("diamicton"),
            SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Till)
        );
    }

    #[test]
    fn maps_specific_periods_to_broad_gameplay_eras() {
        assert_eq!(
            classify_age("lowerCretaceous"),
            Some(GeologicEra::Cretaceous)
        );
        assert_eq!(
            classify_age("http://example/mesoproterozoic"),
            Some(GeologicEra::Precambrian)
        );
        assert_eq!(classify_age("unknown"), None);
    }

    #[test]
    fn age_fallback_is_total_and_has_explicit_evidence() {
        let lithology = SurfaceLithology::Sedimentary(SedimentaryRock::Coal);
        assert_eq!(infer_age(lithology), GeologicEra::Carboniferous);
        assert_eq!(
            GeologicAgeEvidence::Inferred(infer_age(lithology)),
            GeologicAgeEvidence::Inferred(GeologicEra::Carboniferous)
        );
    }

    #[test]
    fn reads_indexed_geopackage_and_parses_missing_age_as_inference() {
        let fixture = Fixture::new();
        let map = GeologyMap::open(&fixture.path).unwrap();
        let profile = map.sample(Point::new(0.0, 0.0)).unwrap().unwrap();
        let SurfaceGeology::Mapped(mapped) = profile else {
            panic!("expected mapped geology")
        };
        assert_eq!(mapped.unit.as_str(), "fixture-unit");
        assert_eq!(
            mapped.setting.lithology,
            GeologicLithologyEvidence::Mapped(SurfaceLithology::Sedimentary(
                SedimentaryRock::Limestone
            ))
        );
        assert_eq!(
            mapped.setting.age,
            GeologicAgeEvidence::Inferred(GeologicEra::Jurassic)
        );
        assert!(map.sample(Point::new(5.0, 5.0)).unwrap().is_none());
    }

    #[test]
    #[ignore = "requires the manually downloaded 675 MB EGDI GeoPackage"]
    fn samples_downloaded_egdi_geopackage() {
        let path = std::env::var_os("EGDI_GEOPACKAGE")
            .map(PathBuf::from)
            .expect("set EGDI_GEOPACKAGE to GeologicUnitView.gpkg");
        let map = GeologyMap::open(&path).unwrap();
        assert!(map.features_read > 200_000);
        let projection = GeologyProjection::new().unwrap();
        let point = projection.project(48.8055, -1.2265).unwrap();
        assert!(map.sample(point).unwrap().is_some());
    }

    struct Fixture {
        path: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "adventuresim-egdi-{}-{nonce}.gpkg",
                std::process::id()
            ));
            let connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "application_id", 0x4750_4b47_i64)
                .unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE gpkg_geometry_columns (table_name TEXT, column_name TEXT, geometry_type_name TEXT, srs_id INTEGER, z INTEGER, m INTEGER);
                     CREATE TABLE GeologicUnitView (
                         fid INTEGER PRIMARY KEY, geom BLOB, id TEXT, geolunitid TEXT, name TEXT,
                         representativeage_uri TEXT, representativeage_title TEXT,
                         representativelithology_uri TEXT, representativelithology_title TEXT
                     );
                     CREATE TABLE rtree_GeologicUnitView_geom (id INTEGER PRIMARY KEY, minx REAL, maxx REAL, miny REAL, maxy REAL);",
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO gpkg_geometry_columns VALUES (?1, ?2, 'MULTISURFACE', 3034, 0, 0)",
                    params![super::TABLE, super::GEOMETRY_COLUMN],
                )
                .unwrap();
            connection.execute(
                "INSERT INTO GeologicUnitView VALUES (1, ?1, 'fixture-unit', NULL, NULL, NULL, 'unknown', NULL, 'limestone')",
                params![square_geopackage_geometry()],
            ).unwrap();
            connection
                .execute(
                    "INSERT INTO rtree_GeologicUnitView_geom VALUES (1, -1.0, 1.0, -1.0, 1.0)",
                    [],
                )
                .unwrap();
            drop(connection);
            Self { path }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn square_geopackage_geometry() -> Vec<u8> {
        let mut bytes = b"GP\0\x03".to_vec();
        bytes.extend_from_slice(&3034_i32.to_le_bytes());
        for coordinate in [-1.0_f64, 1.0, -1.0, 1.0] {
            bytes.extend_from_slice(&coordinate.to_le_bytes());
        }
        bytes.push(1);
        bytes.extend_from_slice(&6_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&5_u32.to_le_bytes());
        for (x, y) in [
            (-1.0_f64, -1.0_f64),
            (1.0, -1.0),
            (1.0, 1.0),
            (-1.0, 1.0),
            (-1.0, -1.0),
        ] {
            bytes.extend_from_slice(&x.to_le_bytes());
            bytes.extend_from_slice(&y.to_le_bytes());
        }
        bytes
    }
}
