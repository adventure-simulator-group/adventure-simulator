use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::Path,
};

use adventuresim_world_schema::{
    CURRENT_INFERENCE_RULES_VERSION, SourceAccess, SourceContentIdentity, SourceLicense,
    SourcePreparation, SourceProvenance, SourceRelease, SourceSpatialCoverage,
    SourceTemporalCoverage, SpatialGridSpec, WORLD_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

const MAX_TEXT: usize = 4_096;
const MAX_NOTICES: usize = 12;
const MAX_SIDECAR_BYTES: u64 = 1024 * 1024;
const MAX_FOREST_INVENTORY_FILES: usize = 4_096;
const VIABUNDUS_RECORD: &str = "https://zenodo.org/api/records/16611998";
const VIABUNDUS_FILES: [&str; 5] = [
    "alternativenames.csv",
    "descriptions.csv",
    "edges.csv",
    "nodes.csv",
    "population.csv",
];

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ViabundusSidecar {
    files: Vec<ViabundusFile>,
    record_url: String,
    version: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ViabundusFile {
    name: String,
    sha256: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
}

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn sha256_json(path: &Path) -> Result<String> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?).map_err(|error| {
        Error::Validation(format!(
            "invalid source manifest {}: {error}",
            path.display()
        ))
    })?;
    let canonical = serde_json::to_vec(&value)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

#[allow(clippy::too_many_arguments)]
fn source(
    id: &str,
    name: &str,
    release: SourceRelease,
    canonical_url: &str,
    doi: Option<&str>,
    license: SourceLicense,
    notices: &[&str],
    access: SourceAccess,
    spatial: SourceSpatialCoverage,
    temporal: SourceTemporalCoverage,
    recipe: &str,
    recipe_version: u32,
    content_identity: SourceContentIdentity,
    notes_markdown: &str,
) -> SourceProvenance {
    SourceProvenance {
        id: id.into(),
        name: name.into(),
        release,
        canonical_url: canonical_url.into(),
        doi: doi.map(str::to_owned),
        license,
        required_notices: notices.iter().map(|notice| (*notice).into()).collect(),
        access,
        spatial,
        temporal,
        preparation: SourcePreparation {
            recipe: recipe.into(),
            version: recipe_version,
        },
        content_identity,
        notes_markdown: notes_markdown.into(),
    }
}

pub(crate) fn viabundus(directory: &Path) -> Result<SourceProvenance> {
    let manifest = directory.join(".viabundus-source.json");
    let identity = if manifest.is_file() {
        let metadata = fs::metadata(&manifest)?;
        if metadata.len() > MAX_SIDECAR_BYTES {
            return Err(Error::Validation(format!(
                "Viabundus sidecar exceeds {MAX_SIDECAR_BYTES} bytes"
            )));
        }
        let mut sidecar: ViabundusSidecar = serde_json::from_slice(&fs::read(&manifest)?)
            .map_err(|error| Error::Validation(format!("invalid Viabundus sidecar: {error}")))?;
        if sidecar.version != "2" || sidecar.record_url != VIABUNDUS_RECORD {
            return Err(Error::Validation(
                "Viabundus sidecar version or record URL is not canonical".into(),
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for file in &sidecar.files {
            if Path::new(&file.name)
                .file_name()
                .and_then(|value| value.to_str())
                != Some(file.name.as_str())
                || !file.name.ends_with(".csv")
                || !names.insert(file.name.clone())
                || file.url.len() > MAX_TEXT
                || !file.url.starts_with("https://")
                || file.sha256.len() != 64
                || !file
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(Error::Validation(
                    "Viabundus sidecar contains an unsafe, duplicate, or malformed file entry"
                        .into(),
                ));
            }
        }
        let expected_names = VIABUNDUS_FILES
            .iter()
            .copied()
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>();
        // The official Viabundus release contains supplementary CSVs.  The
        // importer has an intentionally smaller audited input subset, so the
        // sidecar may inventory additional upstream files as long as it covers
        // every CSV the importer actually consumes.
        if !expected_names.is_subset(&names) {
            return Err(Error::Validation(
                "Viabundus sidecar does not inventory every consumed CSV".into(),
            ));
        }
        sidecar
            .files
            .sort_by(|left, right| left.name.cmp(&right.name));
        let mut fully_sized = true;
        for required in VIABUNDUS_FILES {
            let entry = sidecar
                .files
                .iter()
                .find(|entry| entry.name == required)
                .expect("checked inventory");
            let path = directory.join(required);
            let actual_size = fs::metadata(&path)?.len();
            if entry.size.is_some_and(|size| size != actual_size)
                || sha256_file(&path)? != entry.sha256
            {
                return Err(Error::Validation(format!(
                    "Viabundus sidecar identity mismatch for {required}"
                )));
            }
            fully_sized &= entry.size.is_some();
        }
        if fully_sized {
            SourceContentIdentity::PreparedSnapshotSha256 {
                sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(&sidecar)?)),
            }
        } else {
            SourceContentIdentity::ReleaseBlocked {
                reason: "legacy Viabundus sidecar lacks verified byte sizes".into(),
            }
        }
    } else {
        SourceContentIdentity::ReleaseBlocked {
            reason: "initializer sidecar .viabundus-source.json is absent".into(),
        }
    };
    Ok(source(
        "viabundus-v2",
        "Viabundus Pre-modern Street Map 2",
        SourceRelease::Immutable {
            version: "2".into(),
            released: "2025".into(),
        },
        "https://doi.org/10.5281/zenodo.16611998",
        Some("10.5281/zenodo.16611998"),
        SourceLicense::CcBySa4_0,
        &[
            "Attribute Viabundus and retain the CC BY-SA 4.0 notice on distributed adapted database material.",
            "Fabelgeist applies the conservative CC BY-SA treatment; compatibility with differently licensed combined outputs is unresolved and must not be presented as legal resolution.",
        ],
        SourceAccess::AnonymousDownload,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:4326".into(),
            resolution: "route topology".into(),
            coverage: "pre-modern European transport network".into(),
        },
        SourceTemporalCoverage::Years {
            first: -500,
            last: 2025,
        },
        "viabundus-csv-import",
        1,
        identity,
        "[Viabundus v2](https://doi.org/10.5281/zenodo.16611998), conservatively treated as CC BY-SA 4.0.",
    ))
}

pub(crate) fn elevation() -> SourceProvenance {
    source(
        "copernicus-dem-glo30",
        "Copernicus DEM GLO-30",
        SourceRelease::ReleaseBlocked {
            reason: "manual tile selection has no checked content manifest".into(),
        },
        "https://doi.org/10.5270/ESA-c5d3d65",
        Some("10.5270/ESA-c5d3d65"),
        SourceLicense::CopernicusDem,
        &[
            "Credit: European Union, Copernicus DEM GLO-30.",
            "Produced using Copernicus WorldDEM-30 © DLR e.V. 2010-2014 and © Airbus Defence and Space GmbH 2014-2018 provided under COPERNICUS by the European Union and ESA; all rights reserved.",
            "Neither the European Commission nor ESA is liable for use of Copernicus data and information.",
        ],
        SourceAccess::AuthenticatedDownload,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:4326".into(),
            resolution: "1 arc-second (approximately 30 m)".into(),
            coverage: "global land".into(),
        },
        SourceTemporalCoverage::Timeless,
        "glo30-direct-tile-sampling",
        1,
        SourceContentIdentity::ReleaseBlocked {
            reason: "required GLO-30 tiles are manually selected and not content-pinned".into(),
        },
        "[Copernicus DEM GLO-30](https://doi.org/10.5270/ESA-c5d3d65).",
    )
}

pub(crate) fn hyde35() -> SourceProvenance {
    source(
        "hyde-3-5-c9",
        "History Database of the Global Environment 3.5",
        SourceRelease::Immutable {
            version: "3.5 c9".into(),
            released: "2025-03".into(),
        },
        "https://landuse.sites.uu.nl/hyde-project/",
        None,
        SourceLicense::CcBy3_0,
        &[
            "HYDE 3.5 is licensed CC BY 3.0; retain attribution, the license link, and an indication of Fabelgeist's interpolation and classification changes.",
            "Cite the HYDE project and the HYDE 3.5 release README.",
        ],
        SourceAccess::ManualPreparation,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:4326".into(),
            resolution: "5 arc-minutes".into(),
            coverage: "global land".into(),
        },
        SourceTemporalCoverage::Years {
            first: -10_000,
            last: 2023,
        },
        "hyde35-netcdf-area-interpolation",
        1,
        SourceContentIdentity::ReleaseBlocked {
            reason: "the four manually acquired HYDE 3.5 files are not yet checked into a content inventory".into(),
        },
        "[HYDE 3.5](https://landuse.sites.uu.nl/hyde-project/), sampled as a regional historical reconstruction.",
    )
}

pub(crate) fn forest(directory: &Path) -> Result<SourceProvenance> {
    let marker = directory.join("forest-cover-manifest.json");
    let format = if marker.is_file() {
        let bytes = fs::read(&marker)?;
        Some(crate::sources::forest_cover::validate_prepared_forest_manifest(&bytes, &marker)?)
    } else {
        None
    };
    let (content_identity, notes) = if format
        == Some(crate::sources::forest_cover::PREPARED_FOREST_FORMAT_V2)
    {
        (
            SourceContentIdentity::PreparedSnapshotSha256 {
                sha256: forest_inventory_sha256(directory)?,
            },
            "[Copernicus HRL Forest 2018](https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a), exact pinned-bundle prepared snapshot modified for settlement sampling; a future upstream Process API response is not claimed to be byte-identical.",
        )
    } else {
        (
            SourceContentIdentity::ReleaseBlocked {
                reason: if marker.is_file() {
                    "the local v1 preparation is not bound to the pinned external release descriptor"
                } else {
                    "forest preparation marker and consumed raster inventory are absent"
                }
                .into(),
            },
            "[Copernicus HRL Forest 2018](https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a), local v1 preparation modified for settlement sampling; locally inventoried bytes are not asserted to match the pinned external release or a future upstream Process API response.",
        )
    };
    Ok(source(
        "clms-forest-2018",
        "Copernicus Land Monitoring Service Forest 2018",
        SourceRelease::Immutable {
            version: "2018".into(),
            released: "2018".into(),
        },
        "https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a",
        Some("10.2909/82f93572-9888-47ef-97a1-5cac5985a26a"),
        SourceLicense::CopernicusClms,
        &[
            "© European Union, Copernicus Land Monitoring Service; identify modifications made by Fabelgeist.",
            "Do not imply endorsement by the European Union or the Copernicus programme.",
        ],
        SourceAccess::AuthenticatedDownload,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:4326".into(),
            resolution: "prepared 0.001 degree".into(),
            coverage: "EEA-39".into(),
        },
        SourceTemporalCoverage::ModernProxy { year: 2018 },
        "copernicus-forest-aggregation",
        1,
        content_identity,
        notes,
    ))
}

fn forest_inventory_sha256(directory: &Path) -> Result<String> {
    let mut files = fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(Error::Validation(
                    "forest v2 snapshot contains a non-file entry".into(),
                ));
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Validation("forest v2 filename is not UTF-8".into()))?;
            Ok((name, entry.path()))
        })
        .collect::<Result<Vec<_>>>()?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.is_empty() || files.len() > MAX_FOREST_INVENTORY_FILES {
        return Err(Error::Validation(
            "forest v2 snapshot file count is empty or exceeds its bound".into(),
        ));
    }
    let mut digest = Sha256::new();
    digest.update(b"adventuresim-forest-prepared-inventory-v1");
    for (name, path) in files {
        let size = fs::metadata(&path)?.len();
        let file_sha256 = sha256_file(&path)?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update(size.to_le_bytes());
        digest.update(file_sha256.as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn jung(directory: &Path) -> Result<SourceProvenance> {
    let manifest = directory.join("jung-pnv-manifest.json");
    let identity = if manifest.is_file() {
        SourceContentIdentity::PreparedSnapshotSha256 {
            sha256: sha256_json(&manifest)?,
        }
    } else {
        SourceContentIdentity::ReleaseBlocked {
            reason: "Jung v1.1 manifest is absent at this synthetic/test boundary".into(),
        }
    };
    Ok(source(
        "jung-pnv-1-1",
        "Jung/IIASA European potential vegetation v1.1",
        SourceRelease::Immutable {
            version: "1.1".into(),
            released: "2025-01-10".into(),
        },
        "https://doi.org/10.5281/zenodo.14627466",
        Some("10.5281/zenodo.14627466"),
        SourceLicense::CcBy4_0,
        &[
            "Cite Jung et al. and the Zenodo v1.1 record.",
            "Fabelgeist converts model posterior means and categories into bounded gameplay vegetation evidence; these changes must be identified.",
        ],
        SourceAccess::AnonymousDownload,
        SourceSpatialCoverage::Geographic {
            crs: "ETRS89-LAEA parameters equivalent to EPSG:3035".into(),
            resolution: "1 km".into(),
            coverage: "Europe".into(),
        },
        SourceTemporalCoverage::Years {
            first: 1990,
            last: 2020,
        },
        "jung-cog-area-sampling",
        1,
        identity,
        "[Jung/IIASA PNV v1.1](https://doi.org/10.5281/zenodo.14627466), modified into gameplay classes.",
    ))
}

pub(crate) fn trees(archive: &Path) -> Result<SourceProvenance> {
    Ok(source(
        "eu-trees4f-v2",
        "EU-Trees4F v2 current-climate ensemble",
        SourceRelease::Immutable {
            version: "2".into(),
            released: "2022".into(),
        },
        "https://doi.org/10.6084/m9.figshare.17032328",
        Some("10.6084/m9.figshare.17032328"),
        SourceLicense::Cc0_1_0,
        &[
            "Cite the EU-Trees4F v2 Figshare dataset and associated publication even though the data are dedicated under CC0 1.0.",
        ],
        SourceAccess::AnonymousDownload,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:4326".into(),
            resolution: "30 arc-seconds".into(),
            coverage: "Europe".into(),
        },
        SourceTemporalCoverage::ModernProxy { year: 2020 },
        "eu-trees4f-archive-sampling",
        1,
        SourceContentIdentity::RawSha256 {
            sha256: sha256_file(archive)?,
        },
        "[EU-Trees4F v2](https://doi.org/10.6084/m9.figshare.17032328), CC0 with citation retained.",
    ))
}

pub(crate) fn soil(
    retrieved_at: String,
    prepared_manifest_sha256: String,
) -> Result<SourceProvenance> {
    Ok(source(
        "soilgrids-v2-rolling",
        "ISRIC SoilGrids rolling version 2",
        SourceRelease::Rolling {
            observed_at: retrieved_at,
        },
        "https://www.isric.org/explore/soilgrids",
        Some("10.17027/isric-soilgrids.2"),
        SourceLicense::CcBy4_0,
        &[
            "Credit ISRIC — World Soil Information and cite SoilGrids version 2.",
            "Fabelgeist reprojects, aggregates, and converts SoilGrids predictions into bounded gameplay soil classes; these changes must be identified.",
        ],
        SourceAccess::AnonymousDownload,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:3035".into(),
            resolution: "prepared canonical grid".into(),
            coverage: "Viabundus settlement extent".into(),
        },
        SourceTemporalCoverage::ModernProxy { year: 2020 },
        "soilgrids-wcs-prepare",
        1,
        SourceContentIdentity::PreparedSnapshotSha256 {
            sha256: prepared_manifest_sha256,
        },
        "[ISRIC SoilGrids v2](https://www.isric.org/explore/soilgrids), rolling source captured as a prepared snapshot; raw rolling-latest reacquisition is not reproducible.",
    ))
}

pub(crate) fn geology(_path: &Path) -> SourceProvenance {
    source(
        "egdi-surface-geology-1m",
        "EGDI 1:1 Million pan-European Surface Geology",
        SourceRelease::Immutable {
            version: "EGDI-GE-1M-SURFACE".into(),
            released: "2016-05-04".into(),
        },
        "https://metadata.europe-geology.eu/record/full/5729ffdf-2558-48fc-a5d2-645a0a010855",
        None,
        SourceLicense::CcBy4_0,
        &[
            "Attribute EGDI and contributing national geological surveys; identify Fabelgeist's classification and sampling changes.",
            "For Malta, include Geological Map of the Maltese Islands, Continental Shelf Department, Oil Exploration Directorate, Office of the Prime Minister, Malta, and retain the source disclaimer.",
        ],
        SourceAccess::ManualPreparation,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:3034".into(),
            resolution: "1:1,000,000".into(),
            coverage: "pan-European".into(),
        },
        SourceTemporalCoverage::Timeless,
        "egdi-geopackage-point-sampling",
        1,
        SourceContentIdentity::ReleaseBlocked {
            reason:
                "the manual EGDI GeoPackage has no publisher checksum or checked content sidecar"
                    .into(),
        },
        "[EGDI Surface Geology](https://metadata.europe-geology.eu/record/full/5729ffdf-2558-48fc-a5d2-645a0a010855), modified into gameplay classes; Malta notice applies.",
    )
}

pub(crate) fn religion(path: &Path) -> Result<SourceProvenance> {
    Ok(source(
        "ieg-religion-1544-curated",
        "IEG Maps of Confessional Europe (1500 and 1555)",
        SourceRelease::Curated {
            revision: "adventuresim-ieg-religion-1544-v1".into(),
        },
        "https://www.ieg-maps.uni-mainz.de/mapsp/mapconfession.htm",
        None,
        SourceLicense::RightsReserved,
        &[
            "© IEG Mainz / Andreas Kunz. The source images are not redistributed.",
            "Only Fabelgeist's coarse hand-curated 1544 bounding-region intermediate may be distributed from this repository; it is not a facsimile or precise historical boundary dataset.",
        ],
        SourceAccess::CuratedRepositoryAsset,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:4326".into(),
            resolution: "coarse curated bounding regions".into(),
            coverage: "Europe".into(),
        },
        SourceTemporalCoverage::Year(1544),
        "ieg-map-curation",
        1,
        SourceContentIdentity::CuratedRevision {
            revision: "adventuresim-ieg-religion-1544-v1".into(),
            sha256: sha256_file(path)?,
        },
        "[IEG confessional maps](https://www.ieg-maps.uni-mainz.de/mapsp/mapconfession.htm); source images are not redistributed.",
    ))
}

pub(crate) fn drought(path: &Path) -> Result<SourceProvenance> {
    let derived = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    Ok(source(
        "noaa-owda-v1",
        "NOAA Old World Drought Atlas v1.0",
        if derived {
            SourceRelease::Curated {
                revision: "adventuresim-owda-1544-derived-v1".into(),
            }
        } else {
            SourceRelease::Immutable {
                version: "1.0".into(),
                released: "2015".into(),
            }
        },
        "https://www.ncei.noaa.gov/pub/data/paleo/drought/owda.nc",
        Some("10.25921/rjm6-mq74"),
        SourceLicense::NoaaPublicAccess,
        &[
            "Cite the NOAA/NCEI OWDA dataset DOI 10.25921/rjm6-mq74 and Cook et al. (2015), DOI 10.1126/sciadv.1500561.",
            "Only bounded per-settlement derived values are distributed; do not redistribute the source grid or annual series as part of the compiled world.",
        ],
        if derived {
            SourceAccess::CuratedRepositoryAsset
        } else {
            SourceAccess::AnonymousDownload
        },
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:4326".into(),
            resolution: if derived {
                "bounded per-settlement 20-year profiles".into()
            } else {
                "0.5 degree point grid".into()
            },
            coverage: "Old World drought reconstruction domain".into(),
        },
        SourceTemporalCoverage::Years {
            first: 0,
            last: 2012,
        },
        if derived {
            "owda-bounded-settlement-profile-import"
        } else {
            "owda-20-year-settlement-profile"
        },
        1,
        if derived {
            SourceContentIdentity::CuratedRevision {
                revision: "adventuresim-owda-1544-derived-v1".into(),
                sha256: sha256_file(path)?,
            }
        } else {
            SourceContentIdentity::RawSha256 {
                sha256: sha256_file(path)?,
            }
        },
        "[NOAA/NCEI OWDA v1.0](https://doi.org/10.25921/rjm6-mq74); compiled output is derived-only.",
    ))
}

pub(crate) fn hydrology() -> SourceProvenance {
    source(
        "copernicus-eu-hydro-1-3",
        "Copernicus EU-Hydro River Network Database v1.3",
        SourceRelease::Immutable {
            version: "1.3".into(),
            released: "2020".into(),
        },
        "https://doi.org/10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c",
        Some("10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c"),
        SourceLicense::CopernicusClms,
        &[
            "© European Union, Copernicus Land Monitoring Service; identify Fabelgeist's clipping, classification, and inferred-crossing modifications.",
            "Do not imply endorsement by the European Union or the Copernicus programme.",
        ],
        SourceAccess::AuthenticatedDownload,
        SourceSpatialCoverage::Geographic {
            crs: "EPSG:3035".into(),
            resolution: "vector river network".into(),
            coverage: "EEA-39".into(),
        },
        SourceTemporalCoverage::ModernProxy { year: 2012 },
        "eu-hydro-geopackage-sampling",
        1,
        SourceContentIdentity::ReleaseBlocked {
            reason: "the basin GeoPackage set has no checked inventory or content digest".into(),
        },
        "[Copernicus EU-Hydro v1.3](https://doi.org/10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c), modified for gameplay.",
    )
}

pub(crate) fn canonicalize(sources: &mut [SourceProvenance]) -> Result<()> {
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    for pair in sources.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(Error::Validation(format!(
                "duplicate source manifest id {}",
                pair[0].id
            )));
        }
    }
    for source in sources.iter() {
        validate_source(source)?;
    }
    Ok(())
}

pub(crate) fn validate_source(source: &SourceProvenance) -> Result<()> {
    let valid_text =
        |value: &str| !value.trim().is_empty() && value.len() <= MAX_TEXT && !value.contains('\0');
    let text_fields = [
        &source.id,
        &source.name,
        &source.canonical_url,
        &source.preparation.recipe,
        &source.notes_markdown,
    ];
    if text_fields.iter().any(|value| !valid_text(value)) {
        return Err(Error::Validation(format!(
            "source manifest {} has an empty, oversized, or NUL text field",
            source.id
        )));
    }
    if !source
        .id
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(Error::Validation(format!(
            "source manifest id {} is not canonical kebab-case",
            source.id
        )));
    }
    if !source.canonical_url.starts_with("https://") || source.preparation.version == 0 {
        return Err(Error::Validation(format!(
            "source manifest {} has an invalid URL or recipe version",
            source.id
        )));
    }
    if source.doi.as_ref().is_some_and(|doi| {
        !valid_text(doi) || !doi.starts_with("10.") || doi.chars().any(char::is_whitespace)
    }) {
        return Err(Error::Validation(format!(
            "source manifest {} has an invalid DOI",
            source.id
        )));
    }
    let release_strings: Vec<&str> = match &source.release {
        SourceRelease::Immutable { version, released } => vec![version, released],
        SourceRelease::Curated { revision } => vec![revision],
        SourceRelease::Rolling { observed_at } => vec![observed_at],
        SourceRelease::ReleaseBlocked { reason } => vec![reason],
    };
    if release_strings.iter().any(|value| !valid_text(value)) {
        return Err(Error::Validation(format!(
            "source manifest {} has invalid release metadata",
            source.id
        )));
    }
    if let SourceSpatialCoverage::Geographic {
        crs,
        resolution,
        coverage,
    } = &source.spatial
        && [crs, resolution, coverage]
            .iter()
            .any(|value| !valid_text(value))
    {
        return Err(Error::Validation(format!(
            "source manifest {} has invalid spatial metadata",
            source.id
        )));
    }
    match source.temporal {
        SourceTemporalCoverage::Year(year) | SourceTemporalCoverage::ModernProxy { year }
            if !(-10_000..=10_000).contains(&year) =>
        {
            return Err(Error::Validation(format!(
                "source manifest {} has invalid temporal coverage",
                source.id
            )));
        }
        SourceTemporalCoverage::Years { first, last }
            if first > last || !(-10_000..=10_000).contains(&first) || last > 10_000 =>
        {
            return Err(Error::Validation(format!(
                "source manifest {} has invalid temporal coverage",
                source.id
            )));
        }
        _ => {}
    }
    if source.required_notices.is_empty()
        || source.required_notices.len() > MAX_NOTICES
        || source.required_notices.iter().any(|notice| {
            notice.trim().is_empty() || notice.len() > MAX_TEXT || notice.contains('\0')
        })
    {
        return Err(Error::Validation(format!(
            "source manifest {} has invalid required notices",
            source.id
        )));
    }
    let valid_sha = |sha: &str| {
        sha.len() == 64
            && sha
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    };
    match &source.content_identity {
        SourceContentIdentity::RawSha256 { sha256 }
        | SourceContentIdentity::PreparedSnapshotSha256 { sha256 }
        | SourceContentIdentity::CuratedRevision { sha256, .. }
            if !valid_sha(sha256) =>
        {
            return Err(Error::Validation(format!(
                "source manifest {} has an invalid SHA-256",
                source.id
            )));
        }
        SourceContentIdentity::CuratedRevision { revision, .. } if !valid_text(revision) => {
            return Err(Error::Validation(format!(
                "source manifest {} has an invalid curated revision",
                source.id
            )));
        }
        SourceContentIdentity::UnpinnedRollingObservation { observed_at }
            if !valid_text(observed_at) =>
        {
            return Err(Error::Validation(format!(
                "source manifest {} has an invalid observation",
                source.id
            )));
        }
        SourceContentIdentity::ReleaseBlocked { reason } if !valid_text(reason) => {
            return Err(Error::Validation(format!(
                "source manifest {} has an invalid blocker",
                source.id
            )));
        }
        _ => {}
    }
    Ok(())
}

#[derive(Serialize)]
struct Identity<'a> {
    schema_version: u32,
    inference_rules_version: u32,
    world_year: i32,
    spatial_grid: SpatialGridSpec,
    sources: &'a [SourceProvenance],
}

pub(crate) fn digest(
    year: i32,
    grid: SpatialGridSpec,
    sources: &[SourceProvenance],
) -> Result<String> {
    let bytes = serde_json::to_vec(&Identity {
        schema_version: WORLD_SCHEMA_VERSION,
        inference_rules_version: CURRENT_INFERENCE_RULES_VERSION,
        world_year: year,
        spatial_grid: grid,
        sources,
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_world_schema::{GridCellSizeMeters, SourceContentIdentity};

    fn fixture() -> SourceProvenance {
        hydrology()
    }

    fn viabundus_fixture(directory: &Path) -> ViabundusSidecar {
        fs::create_dir_all(directory).unwrap();
        let files = VIABUNDUS_FILES
            .iter()
            .map(|name| {
                fs::write(directory.join(name), name.as_bytes()).unwrap();
                ViabundusFile {
                    name: (*name).into(),
                    sha256: format!("{:x}", Sha256::digest(name.as_bytes())),
                    url: format!("https://example.invalid/{name}"),
                    size: Some(name.len() as u64),
                }
            })
            .collect();
        ViabundusSidecar {
            files,
            record_url: VIABUNDUS_RECORD.into(),
            version: "2".into(),
        }
    }

    fn write_viabundus(directory: &Path, sidecar: &ViabundusSidecar) {
        fs::write(
            directory.join(".viabundus-source.json"),
            serde_json::to_vec(sidecar).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn canonicalization_is_order_independent_and_rejects_duplicates() {
        let mut first = vec![hyde35(), elevation()];
        let mut second = vec![elevation(), hyde35()];
        canonicalize(&mut first).unwrap();
        canonicalize(&mut second).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            digest(1544, SpatialGridSpec::default(), &first).unwrap(),
            digest(1544, SpatialGridSpec::default(), &second).unwrap()
        );

        first.push(first[0].clone());
        assert!(
            canonicalize(&mut first)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }

    #[test]
    fn identity_changes_for_every_manifest_field_and_build_dimension() {
        let original = fixture();
        let baseline = digest(
            1544,
            SpatialGridSpec::default(),
            std::slice::from_ref(&original),
        )
        .unwrap();
        let mut variants = Vec::new();
        macro_rules! changed {
            ($field:ident, $value:expr) => {{
                let mut item = original.clone();
                item.$field = $value;
                variants.push(item);
            }};
        }
        changed!(id, "different-id".into());
        changed!(name, "different name".into());
        changed!(
            release,
            SourceRelease::Curated {
                revision: "different".into()
            }
        );
        changed!(canonical_url, "https://example.invalid/different".into());
        changed!(doi, None);
        changed!(license, SourceLicense::CcBy4_0);
        changed!(required_notices, vec!["different notice".into()]);
        changed!(access, SourceAccess::ManualPreparation);
        changed!(spatial, SourceSpatialCoverage::NotApplicable);
        changed!(temporal, SourceTemporalCoverage::Timeless);
        changed!(
            preparation,
            SourcePreparation {
                recipe: "different".into(),
                version: 2
            }
        );
        changed!(
            content_identity,
            SourceContentIdentity::UnpinnedRollingObservation {
                observed_at: "different".into()
            }
        );
        changed!(notes_markdown, "different notes".into());
        for variant in variants {
            assert_ne!(
                baseline,
                digest(1544, SpatialGridSpec::default(), &[variant]).unwrap()
            );
        }
        assert_ne!(
            baseline,
            digest(
                1545,
                SpatialGridSpec::default(),
                std::slice::from_ref(&original)
            )
            .unwrap()
        );
        assert_ne!(
            baseline,
            digest(
                1544,
                SpatialGridSpec::new(GridCellSizeMeters::new(250).unwrap()),
                std::slice::from_ref(&original)
            )
            .unwrap()
        );
    }

    #[test]
    fn unpinned_and_blocked_identities_are_not_reproducible() {
        assert!(
            !SourceContentIdentity::UnpinnedRollingObservation {
                observed_at: "2026-01-01".into()
            }
            .is_reproducible()
        );
        assert!(
            !SourceContentIdentity::ReleaseBlocked {
                reason: "manual".into()
            }
            .is_reproducible()
        );
        assert!(
            SourceContentIdentity::RawSha256 {
                sha256: "a".repeat(64)
            }
            .is_reproducible()
        );
    }

    #[test]
    fn operational_notices_cover_all_integrated_sources() {
        let sources = [
            viabundus(Path::new("missing")).unwrap(),
            elevation(),
            hyde35(),
            forest(Path::new("missing")).unwrap(),
            geology(Path::new("manual.gpkg")),
            religion(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/world-data/ieg-religion-1544.csv")
                    .as_path(),
            )
            .unwrap(),
            hydrology(),
        ];
        let joined = sources
            .iter()
            .flat_map(|source| &source.required_notices)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        for required in [
            "CC BY-SA",
            "liable",
            "HYDE 3.5 is licensed CC BY 3.0",
            "modifications",
            "Malta",
            "not redistributed",
            "endorsement",
        ] {
            assert!(
                joined.contains(required),
                "missing notice phrase {required}"
            );
        }
    }

    #[test]
    fn fixture_digest_is_stable() {
        assert_eq!(
            digest(1544, SpatialGridSpec::default(), &[fixture()]).unwrap(),
            "e06cf6c350bb3b109c9be774c9f169a94952bdf21943015965a1e7dd91e5830d"
        );
    }

    #[test]
    fn forest_metadata_distinguishes_local_v1_and_pinned_v2() {
        let root = std::env::temp_dir().join(format!(
            "adventuresim-forest-manifest-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        fs::write(
            root.join("forest-cover-manifest.json"),
            br#"{"format":"adventuresim-copernicus-forest-2018-v1"}"#,
        )
        .unwrap();
        let local = forest(&root).unwrap();
        assert!(matches!(
            local.content_identity,
            SourceContentIdentity::ReleaseBlocked { .. }
        ));
        assert!(local.notes_markdown.contains("local v1 preparation"));

        fs::write(
            root.join("forest-cover-manifest.json"),
            br#"{"format":"adventuresim-copernicus-forest-2018-v2"}"#,
        )
        .unwrap();
        fs::write(root.join("TCD_N53_E009.tif"), b"tcd").unwrap();
        fs::write(root.join("DLT_N53_E009.tif"), b"dlt").unwrap();
        let pinned = forest(&root).unwrap();
        let SourceContentIdentity::PreparedSnapshotSha256 { sha256: first } =
            pinned.content_identity
        else {
            panic!("v2 must retain an exact prepared-snapshot identity");
        };
        assert!(pinned.notes_markdown.contains("exact pinned-bundle"));
        assert!(
            pinned
                .notes_markdown
                .contains("not claimed to be byte-identical")
        );

        fs::write(root.join("TCD_N53_E009.tif"), b"changed").unwrap();
        let SourceContentIdentity::PreparedSnapshotSha256 { sha256: second } =
            forest(&root).unwrap().content_identity
        else {
            panic!("v2 must retain an exact prepared-snapshot identity");
        };
        assert_ne!(first, second);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn viabundus_sidecar_is_bounded_strict_and_content_verified() {
        let root = std::env::temp_dir().join(format!(
            "adventuresim-viabundus-manifest-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut sidecar = viabundus_fixture(&root);
        write_viabundus(&root, &sidecar);
        assert!(viabundus(&root).unwrap().content_identity.is_reproducible());

        sidecar.files[0].sha256 = "0".repeat(64);
        write_viabundus(&root, &sidecar);
        assert!(
            viabundus(&root)
                .unwrap_err()
                .to_string()
                .contains("mismatch")
        );
        sidecar = viabundus_fixture(&root);
        sidecar.files.push(ViabundusFile {
            name: sidecar.files[0].name.clone(),
            sha256: "a".repeat(64),
            url: "https://example.invalid/duplicate".into(),
            size: Some(1),
        });
        write_viabundus(&root, &sidecar);
        assert!(
            viabundus(&root)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        sidecar = viabundus_fixture(&root);
        sidecar.files.push(ViabundusFile {
            name: "unused.csv".into(),
            sha256: "a".repeat(64),
            url: "https://example.invalid/unused.csv".into(),
            size: Some(1),
        });
        write_viabundus(&root, &sidecar);
        assert!(viabundus(&root).unwrap().content_identity.is_reproducible());
        sidecar = viabundus_fixture(&root);
        sidecar.files[0].name = "../nodes.csv".into();
        write_viabundus(&root, &sidecar);
        assert!(viabundus(&root).unwrap_err().to_string().contains("unsafe"));
        fs::write(
            root.join(".viabundus-source.json"),
            vec![b' '; MAX_SIDECAR_BYTES as usize + 1],
        )
        .unwrap();
        assert!(
            viabundus(&root)
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn geology_identity_is_path_independent() {
        assert_eq!(
            geology(Path::new("one/a.gpkg")),
            geology(Path::new("two/b.gpkg"))
        );
    }

    #[test]
    fn nested_manifest_strings_and_temporal_ranges_fail_closed() {
        let mut source = fixture();
        source.release = SourceRelease::Immutable {
            version: String::new(),
            released: "2020".into(),
        };
        assert!(validate_source(&source).is_err());
        source = fixture();
        source.doi = Some("10.bad\0doi".into());
        assert!(validate_source(&source).is_err());
        source = fixture();
        source.spatial = SourceSpatialCoverage::Geographic {
            crs: "x".repeat(MAX_TEXT + 1),
            resolution: "1 km".into(),
            coverage: "Europe".into(),
        };
        assert!(validate_source(&source).is_err());
        source = fixture();
        source.temporal = SourceTemporalCoverage::Years {
            first: 2020,
            last: 1990,
        };
        assert!(validate_source(&source).is_err());

        let mut value = serde_json::to_value(SourceRelease::Immutable {
            version: "1".into(),
            released: "2020".into(),
        })
        .unwrap();
        value["immutable"]["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<SourceRelease>(value).is_err());
    }
}
