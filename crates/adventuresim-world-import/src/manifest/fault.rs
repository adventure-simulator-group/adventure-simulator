use std::path::Path;

use adventuresim_world_schema::{
    SourceAccess, SourceContentIdentity, SourceLicense, SourceProvenance, SourceRelease,
    SourceSpatialCoverage, SourceTemporalCoverage,
};

use super::sha256_file;
use crate::Result;

pub(crate) fn source(path: &Path) -> Result<SourceProvenance> {
    Ok(super::source(
        "hike-fault-db-v17b",
        "HIKE European Fault Database",
        SourceRelease::Immutable {
            version: "17b".into(),
            released: "2021-10-26".into(),
        },
        "https://egdi.geology.cz/record/basic/5edf7bd4-9270-4188-b69d-7ddd0a010833",
        None,
        SourceLicense::RightsReserved,
        &[
            "Attribute the HIKE project, EGDI, BGR, and the contributing national geological surveys under their contributor-specific terms.",
            "Fabelgeist clips, deduplicates, and simplifies source fault traces into a terrain-generation prior; the result does not represent seismic hazard.",
        ],
        SourceAccess::AnonymousDownload,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:3034".into(),
            resolution: "contributor scales from 1:25,000 to 1:2,500,000".into(),
            coverage: "European fault database clipped to the playable bounds".into(),
        },
        SourceTemporalCoverage::Timeless,
        "hike-fault-geopackage-clip",
        1,
        SourceContentIdentity::RawSha256 {
            sha256: sha256_file(path)?,
        },
        "HIKE European Fault Database v17b, clipped and normalized for deterministic terrain generation.",
    ))
}
