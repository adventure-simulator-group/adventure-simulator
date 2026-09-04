use super::*;

pub(super) struct GeologyMap {
    pub(super) connection: Connection,
    pub(super) path: PathBuf,
    pub(super) features_read: usize,
}

impl GeologyMap {
    pub(super) fn open(path: &Path) -> Result<Self> {
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
        validate_spatial_index(&connection, path)?;
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

    pub(super) fn candidates(&self, point: Point<f64>) -> Result<Vec<RawGeologyFeature>> {
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
        rows.map(|row| {
            row.map_err(|source| Error::GeoPackage {
                path: self.path.clone(),
                source,
            })
        })
        .collect()
    }

    pub(super) fn sample(&self, point: Point<f64>) -> Result<Option<SurfaceGeology>> {
        for feature in self.candidates(point)? {
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

fn validate_spatial_index(connection: &Connection, path: &Path) -> Result<()> {
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
    Ok(())
}
