//! EGDI 1:1 million pan-European surface-geology sampling.

use std::path::{Path, PathBuf};

use adventuresim_world_schema::{
    GeologicAgeEvidence, GeologicEra, GeologicLithologyEvidence, GeologicSetting, GeologicUnitId,
    IgneousRock, InferredGeologicSetting, MappedSurfaceGeology, MetamorphicRock, MixedLithology,
    SedimentaryRock, SoilProfile, SoilSubstrate, SourceProvenance, SurfaceGeology,
    SurfaceLithology, UnconsolidatedDeposit,
};
use geo::{Contains, Geometry, Point};
use geozero::{ToGeo, wkb::GpkgWkb};
use proj4rs::{proj::Proj, transform::transform};
use rusqlite::{Connection, OpenFlags, params};

use crate::{
    Error, Result,
    draft::{GeologySettlementDraft, SoilSettlementDraft, WorldDraft, push_source_note},
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
) -> Result<WorldDraft<GeologySettlementDraft>> {
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
) -> Result<WorldDraft<GeologySettlementDraft>> {
    if profiles.len() != draft.settlements.len() {
        return Err(Error::Validation(
            "geology profiles do not match settlements".into(),
        ));
    }
    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(profiles)
        .map(|(mut soil, geology)| {
            push_source_note(
                &mut soil,
                match &geology {
                    SurfaceGeology::Mapped(_) => "**[EGDI pan-European Surface Geology](https://doi.org/10.22008/y9hj-va55):** Surface lithology and age come from containment in the indexed aggregate geologic-unit layer; missing source attributes may use the mapped record's explicit deterministic evidence classifications.",
                    SurfaceGeology::Inferred(_) => "**EGDI geology fallback:** No usable geologic unit covered the settlement, so a plausible geologic setting is deterministically inferred from the soil profile.",
                },
            );
            GeologySettlementDraft { soil, geology }
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
    Ok(WorldDraft {
        year: draft.year,
        world_bounds: draft.world_bounds,
        sources: draft.sources,
        road_types: draft.road_types,
        nodes: draft.nodes,
        edges: draft.edges,
        settlement_aliases: draft.settlement_aliases,
        settlement_descriptions: draft.settlement_descriptions,
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
        let (organization, organization_srs): (String, i64) = connection
            .query_row(
                "SELECT organization, organization_coordsys_id FROM gpkg_spatial_ref_sys WHERE srs_id = ?1",
                params![EXPECTED_SRS],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        if !organization.eq_ignore_ascii_case("EPSG") || organization_srs != EXPECTED_SRS {
            return Err(Error::Validation(format!(
                "{} binds GeoPackage SRS {EXPECTED_SRS} to {organization}:{organization_srs}; expected EPSG:{EXPECTED_SRS}",
                path.display()
            )));
        }
        let contents_srs: i64 = connection
            .query_row(
                "SELECT srs_id FROM gpkg_contents WHERE table_name = ?1 AND data_type = 'features'",
                params![TABLE],
                |row| row.get(0),
            )
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let rtree_registration: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM gpkg_extensions WHERE table_name = ?1 AND column_name = ?2 AND extension_name = 'gpkg_rtree_index'",
                params![TABLE, GEOMETRY_COLUMN],
                |row| row.get(0),
            )
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let rtree_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'rtree_GeologicUnitView_geom'",
                [],
                |row| row.get(0),
            )
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        // GeoPackage flags are the fourth byte: bits 4/5 are empty and
        // standard-vs-extended geometry, bits 1..=3 select the envelope, and
        // bit 0 is byte order. The first hex nibble is therefore 0/2 for a
        // non-empty geometry or 1/3 for an empty one; the second is 0..=9.
        let missing_rtree_entries: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM GeologicUnitView g LEFT JOIN rtree_GeologicUnitView_geom r ON r.id = g.fid WHERE g.geom IS NOT NULL AND substr(hex(g.geom), 7, 1) IN ('0', '2') AND r.id IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let invalid_geometry_headers: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM GeologicUnitView WHERE geom IS NULL OR length(geom) < 8 OR substr(geom, 1, 2) != X'4750' OR substr(hex(geom), 7, 1) NOT IN ('0', '1', '2', '3') OR substr(hex(geom), 8, 1) NOT IN ('0', '1', '2', '3', '4', '5', '6', '7', '8', '9')",
                [],
                |row| row.get(0),
            )
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        let orphan_rtree_entries: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM rtree_GeologicUnitView_geom r LEFT JOIN GeologicUnitView g ON g.fid = r.id WHERE g.fid IS NULL OR g.geom IS NULL OR substr(hex(g.geom), 7, 1) IN ('1', '3')",
                [],
                |row| row.get(0),
            )
            .map_err(|source| Error::GeoPackage {
                path: path.to_path_buf(),
                source,
            })?;
        if contents_srs != EXPECTED_SRS
            || rtree_registration != 1
            || !is_expected_rtree_sql(&rtree_sql)
            || invalid_geometry_headers != 0
            || missing_rtree_entries != 0
            || orphan_rtree_entries != 0
        {
            return Err(Error::Validation(format!(
                "{} does not register GeologicUnitView as an EPSG:{EXPECTED_SRS} feature layer with a virtual GeoPackage R-tree",
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

fn is_expected_rtree_sql(sql: &str) -> bool {
    let normalized = sql
        .chars()
        .filter(|character| {
            !character.is_ascii_whitespace() && !matches!(character, '"' | '`' | '[' | ']' | ';')
        })
        .flat_map(char::to_lowercase)
        .collect::<String>();
    normalized == "createvirtualtablertree_geologicunitview_geomusingrtree(id,minx,maxx,miny,maxy)"
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
    let lowercase = value.to_ascii_lowercase();
    value.is_empty()
        || matches!(
            lowercase.as_str(),
            "unknown" | "nil" | "missing" | "notavailable" | "not available"
        )
        || lowercase.contains("/nil/ogc/")
        || lowercase.contains("voidreasonvalue/")
}

fn classify_lithology(value: &str) -> SurfaceLithology {
    use IgneousRock as I;
    use MetamorphicRock as M;
    use SedimentaryRock as S;
    use SurfaceLithology as L;
    use UnconsolidatedDeposit as U;
    let value = value.to_ascii_lowercase();
    let has = |needle: &str| value.contains(needle);
    if has("limestone")
        || has("carbonatesedimentary")
        || has("carbonate sedimentary")
        || has("carbonateooze")
        || has("carbonatemud")
        || has("travertine")
    {
        L::Sedimentary(S::Limestone)
    } else if has("dolom") {
        L::Sedimentary(S::Dolostone)
    } else if has("chalk") {
        L::Sedimentary(S::Chalk)
    } else if has("marl") {
        L::Sedimentary(S::Marl)
    } else if has("sandstone") || has("arenite") || has("wacke") {
        L::Sedimentary(S::Sandstone)
    } else if has("siltstone") {
        L::Sedimentary(S::Siltstone)
    } else if has("mudstone") || has("claystone") {
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
    } else if has("gabbro") || has("dolerit") {
        L::Igneous(I::Gabbro)
    } else if has("basalt") {
        L::Igneous(I::Basalt)
    } else if has("andesite") {
        L::Igneous(I::Andesite)
    } else if has("rhyolit") {
        L::Igneous(I::Rhyolite)
    } else if has("tuff") {
        L::Igneous(I::Tuff)
    } else if has("plutonic")
        || has("intrusive")
        || has("peridotite")
        || has("tonalite")
        || has("syenit")
        || has("monzonite")
        || has("pyroxenite")
        || has("anorthosit")
        || has("porphyry")
        || has("phaneritic")
    {
        L::Igneous(I::OtherPlutonic)
    } else if has("volcanic")
        || has("extrusive")
        || has("lava")
        || has("pyroclastic")
        || has("basanite")
        || has("tephrite")
        || has("spilite")
        || has("trachyt")
        || has("phonolite")
        || has("dacite")
        || has("komatiit")
        || has("carbonatite")
    {
        L::Igneous(I::OtherVolcanic)
    } else if has("igneous") {
        L::Igneous(I::OtherIgneous)
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
    } else if has("metamorph")
        || has("migmatite")
        || has("granulite")
        || has("serpentinite")
        || has("mylonit")
        || has("eclogite")
        || has("skarn")
        || has("hornfels")
        || has("phyllonite")
    {
        L::Metamorphic(M::OtherMetamorphic)
    } else if has("clay") {
        L::Unconsolidated(U::Clay)
    } else if has("silt") || has("silicatemud") || value == "mud" {
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
    } else if has("sediment") || has("unconsolidated") || has("residualmaterial") {
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
    let has_any = |needles: &[&str]| needles.iter().any(|needle| has(needle));
    if is_missing(&value) {
        None
    } else if has_any(&[
        "quaternary",
        "holocene",
        "pleistocene",
        "ionian",
        "calabrian",
        "gelasian",
    ]) {
        Some(E::Quaternary)
    } else if has_any(&[
        "neogene",
        "miocene",
        "pliocene",
        "aquitanian",
        "burdigalian",
        "langhian",
        "serravallian",
        "tortonian",
        "messinian",
        "zanclean",
        "piacenzian",
    ]) {
        Some(E::Neogene)
    } else if has_any(&[
        "paleogene",
        "palaeogene",
        "eocene",
        "oligocene",
        "paleocene",
        "danian",
        "selandian",
        "thanetian",
        "ypresian",
        "lutetian",
        "bartonian",
        "priabonian",
        "rupelian",
        "chattian",
    ]) {
        Some(E::Paleogene)
    } else if has_any(&[
        "cretaceous",
        "berriasian",
        "valanginian",
        "hauterivian",
        "barremian",
        "aptian",
        "albian",
        "cenomanian",
        "turonian",
        "coniacian",
        "santonian",
        "campanian",
        "maastrichtian",
    ]) {
        Some(E::Cretaceous)
    } else if has_any(&[
        "jurassic",
        "hettangian",
        "sinemurian",
        "pliensbachian",
        "toarcian",
        "aalenian",
        "bajocian",
        "bathonian",
        "callovian",
        "oxfordian",
        "kimmeridgian",
        "tithonian",
    ]) {
        Some(E::Jurassic)
    } else if has_any(&[
        "triassic",
        "induan",
        "olenekian",
        "anisian",
        "ladinian",
        "carnian",
        "norian",
        "rhaetian",
    ]) {
        Some(E::Triassic)
    } else if has_any(&["permian", "cisuralian", "guadalupian", "lopingian"]) {
        Some(E::Permian)
    } else if has_any(&[
        "carboniferous",
        "mississippian",
        "pennsylvanian",
        "tournaisian",
        "visean",
        "serpukhovian",
        "bashkirian",
        "moscovian",
        "kasimovian",
        "gzhelian",
    ]) {
        Some(E::Carboniferous)
    } else if has_any(&[
        "devonian",
        "lochkovian",
        "pragian",
        "emsian",
        "eifelian",
        "givetian",
        "frasnian",
        "famennian",
    ]) {
        Some(E::Devonian)
    } else if has_any(&[
        "silurian",
        "llandovery",
        "rhuddanian",
        "aeronian",
        "telychian",
        "wenlock",
        "ludlow",
        "pridoli",
    ]) {
        Some(E::Silurian)
    } else if has_any(&[
        "ordovician",
        "tremadocian",
        "floian",
        "dapingian",
        "darriwilian",
        "sandbian",
        "katian",
        "hirnantian",
    ]) {
        Some(E::Ordovician)
    } else if has_any(&[
        "precambrian",
        "proterozoic",
        "archaean",
        "archean",
        "hadaean",
        "ediacaran",
        "cryogenian",
        "tonian",
        "stenian",
        "ectasian",
        "calymmian",
        "statherian",
        "orosirian",
        "rhyacian",
        "siderian",
    ]) {
        Some(E::Precambrian)
    } else if has_any(&[
        "cambrian",
        "terreneuvian",
        "furongian",
        "drumian",
        "stage2",
        "stage4",
        "stage5",
        "stage10",
        "series2",
        "series3",
    ]) {
        Some(E::Cambrian)
    } else if has("cenozoic") {
        Some(E::Cenozoic)
    } else if has("mesozoic") {
        Some(E::Mesozoic)
    } else if has("paleozoic") || has("palaeozoic") {
        Some(E::Paleozoic)
    } else if has("phanerozoic") {
        Some(E::Phanerozoic)
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

fn infer_setting(settlement: &SoilSettlementDraft) -> InferredGeologicSetting {
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
    InferredGeologicSetting {
        lithology,
        age: infer_age(lithology),
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
        assert_eq!(
            classify_lithology("claystone"),
            SurfaceLithology::Sedimentary(SedimentaryRock::Mudstone)
        );
        assert_eq!(
            classify_lithology("ultramaficIgneousRock"),
            SurfaceLithology::Igneous(IgneousRock::OtherIgneous)
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
        assert_eq!(classify_age("precambrian"), Some(GeologicEra::Precambrian));
        assert_eq!(classify_age("visean"), Some(GeologicEra::Carboniferous));
        assert_eq!(classify_age("rhaetian"), Some(GeologicEra::Triassic));
        assert_eq!(classify_age("unknown"), None);
        assert_eq!(
            classify_age("http://inspire.ec.europa.eu/codelist/VoidReasonValue/Unpopulated"),
            None
        );
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
    fn rejects_a_registered_but_incomplete_spatial_index() {
        let fixture = Fixture::new();
        let connection = Connection::open(&fixture.path).unwrap();
        connection
            .execute("DELETE FROM rtree_GeologicUnitView_geom", [])
            .unwrap();
        drop(connection);
        assert!(matches!(
            GeologyMap::open(&fixture.path),
            Err(crate::Error::Validation(message)) if message.contains("virtual GeoPackage R-tree")
        ));
    }

    #[test]
    fn rejects_an_empty_geometry_in_the_spatial_index() {
        let fixture = Fixture::new();
        let connection = Connection::open(&fixture.path).unwrap();
        connection.execute(
            "INSERT INTO GeologicUnitView VALUES (2, ?1, 'empty-unit', NULL, NULL, NULL, 'unknown', NULL, 'unknown')",
            params![empty_geopackage_geometry()],
        ).unwrap();
        connection
            .execute(
                "INSERT INTO rtree_GeologicUnitView_geom VALUES (2, 0.0, 0.0, 0.0, 0.0)",
                [],
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            GeologyMap::open(&fixture.path),
            Err(crate::Error::Validation(message)) if message.contains("virtual GeoPackage R-tree")
        ));
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
        let mut statement = map
            .connection
            .prepare(
                "SELECT DISTINCT representativeage_title, representativeage_uri FROM GeologicUnitView",
            )
            .unwrap();
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })
            .unwrap();
        for row in rows {
            let (title, uri) = row.unwrap();
            let label = super::preferred_label(title, uri);
            if !super::is_missing(&label) {
                assert!(
                    super::classify_age(&label).is_some(),
                    "unparsed EGDI age codelist term {label:?}"
                );
            }
        }
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
                    "CREATE TABLE gpkg_spatial_ref_sys (srs_name TEXT, srs_id INTEGER PRIMARY KEY, organization TEXT, organization_coordsys_id INTEGER, definition TEXT, description TEXT);
                     CREATE TABLE gpkg_contents (table_name TEXT PRIMARY KEY, data_type TEXT, identifier TEXT, description TEXT DEFAULT '', last_change TEXT, min_x REAL, min_y REAL, max_x REAL, max_y REAL, srs_id INTEGER);
                     CREATE TABLE gpkg_geometry_columns (table_name TEXT, column_name TEXT, geometry_type_name TEXT, srs_id INTEGER, z INTEGER, m INTEGER);
                     CREATE TABLE gpkg_extensions (table_name TEXT, column_name TEXT, extension_name TEXT, definition TEXT, scope TEXT);
                     CREATE TABLE GeologicUnitView (
                         fid INTEGER PRIMARY KEY, geom BLOB, id TEXT, geolunitid TEXT, name TEXT,
                         representativeage_uri TEXT, representativeage_title TEXT,
                         representativelithology_uri TEXT, representativelithology_title TEXT
                     );
                     CREATE VIRTUAL TABLE rtree_GeologicUnitView_geom USING rtree(id, minx, maxx, miny, maxy);",
                )
                .unwrap();
            connection.execute(
                "INSERT INTO gpkg_spatial_ref_sys VALUES ('ETRS89-extended / LCC Europe', 3034, 'EPSG', 3034, 'fixture', '')",
                [],
            ).unwrap();
            connection.execute(
                "INSERT INTO gpkg_contents (table_name, data_type, identifier, srs_id) VALUES (?1, 'features', ?1, 3034)",
                params![super::TABLE],
            ).unwrap();
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
            connection.execute(
                "INSERT INTO gpkg_extensions VALUES (?1, ?2, 'gpkg_rtree_index', 'http://www.geopackage.org/spec120/#extension_rtree', 'write-only')",
                params![super::TABLE, super::GEOMETRY_COLUMN],
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

    fn empty_geopackage_geometry() -> Vec<u8> {
        let mut bytes = b"GP\0\x11".to_vec();
        bytes.extend_from_slice(&3034_i32.to_le_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&6_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes
    }
}
