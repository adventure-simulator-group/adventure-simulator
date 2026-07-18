//! IEG legal-religion reconstruction through a checked, map-derived 1544
//! regional intermediate.

use std::collections::HashSet;
use std::path::Path;

use adventuresim_world_schema::{
    CatholicLutheranChurch, CatholicReformedChurch, LutheranReformedChurch, OfficialReligion,
    SettlementReligiousStatus, SourceProvenance, WesternChristianArrangement,
};
use serde::Deserialize;

use crate::{
    Error, Result,
    draft::{GeologySettlementDraft, ReligionSettlementDraft, WorldDraft, push_source_note},
};

const SOURCE_NAME: &str = "IEG Maps of Confessional Europe (1500 and 1555)";
const SOURCE_URL: &str = "https://www.ieg-maps.uni-mainz.de/mapsp/mapconfession.htm";
const SOURCE_LICENSE: &str =
    "IEG map rights: © IEG Mainz / Andreas Kunz; project-curated intermediate";
const SUPPORTED_YEAR: i32 = 1544;

#[derive(Debug, Deserialize)]
struct RawRegion {
    priority: u16,
    region: String,
    min_latitude: f64,
    max_latitude: f64,
    min_longitude: f64,
    max_longitude: f64,
    status: String,
    religions: String,
    church: String,
}

#[derive(Debug)]
struct Region {
    priority: u16,
    name: String,
    min_latitude: f64,
    max_latitude: f64,
    min_longitude: f64,
    max_longitude: f64,
    status: SettlementReligiousStatus,
}

impl Region {
    fn parse(path: &Path, raw: RawRegion) -> Result<Self> {
        let invalid = |field, value: &str, message: &str| Error::InvalidField {
            path: path.to_path_buf(),
            field,
            value: value.into(),
            message: message.into(),
        };
        let name = raw.region.trim().to_owned();
        if name.is_empty() {
            return Err(invalid(
                "region",
                &raw.region,
                "region name cannot be empty",
            ));
        }
        for (field, value, min, max) in [
            ("min_latitude", raw.min_latitude, -90.0, 90.0),
            ("max_latitude", raw.max_latitude, -90.0, 90.0),
            ("min_longitude", raw.min_longitude, -180.0, 180.0),
            ("max_longitude", raw.max_longitude, -180.0, 180.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(invalid(
                    field,
                    &value.to_string(),
                    "coordinate is outside its valid range",
                ));
            }
        }
        if raw.min_latitude >= raw.max_latitude || raw.min_longitude >= raw.max_longitude {
            return Err(invalid(
                "bounds",
                &name,
                "region must have positive width and height",
            ));
        }

        let status = parse_status(path, &raw)?;
        Ok(Self {
            priority: raw.priority,
            name,
            min_latitude: raw.min_latitude,
            max_latitude: raw.max_latitude,
            min_longitude: raw.min_longitude,
            max_longitude: raw.max_longitude,
            status,
        })
    }

    fn contains(&self, latitude: f64, longitude: f64) -> bool {
        (self.min_latitude..=self.max_latitude).contains(&latitude)
            && (self.min_longitude..=self.max_longitude).contains(&longitude)
    }
}

fn parse_status(path: &Path, raw: &RawRegion) -> Result<SettlementReligiousStatus> {
    let invalid = |field, value: &str, message: &str| Error::InvalidField {
        path: path.to_path_buf(),
        field,
        value: value.into(),
        message: message.into(),
    };
    let religions = raw.religions.trim();
    let church = raw.church.trim();
    match raw.status.trim() {
        "established" => {
            if !church.is_empty() {
                return Err(invalid(
                    "church",
                    church,
                    "established rows derive their church from religions",
                ));
            }
            Ok(SettlementReligiousStatus::Established {
                religion: parse_religion(path, "religions", religions)?,
            })
        }
        "locally_determined" => {
            if !religions.is_empty() {
                return Err(invalid(
                    "religions",
                    religions,
                    "locally determined rows specify only their selected church",
                ));
            }
            Ok(SettlementReligiousStatus::LocallyDetermined {
                church: parse_religion(path, "church", church)?,
            })
        }
        "parity" | "multi_confessional" => {
            let arrangement = parse_arrangement(path, religions, church)?;
            Ok(if raw.status.trim() == "parity" {
                SettlementReligiousStatus::Parity { arrangement }
            } else {
                SettlementReligiousStatus::MultiConfessional { arrangement }
            })
        }
        value => Err(invalid(
            "status",
            value,
            "expected established, parity, multi_confessional, or locally_determined",
        )),
    }
}

fn parse_religion(path: &Path, field: &'static str, value: &str) -> Result<OfficialReligion> {
    match value {
        "roman_catholic" => Ok(OfficialReligion::RomanCatholic),
        "lutheran" => Ok(OfficialReligion::Lutheran),
        "reformed" => Ok(OfficialReligion::Reformed),
        "anglican" => Ok(OfficialReligion::Anglican),
        "protestant_unspecified" => Ok(OfficialReligion::ProtestantUnspecified),
        "eastern_orthodox" => Ok(OfficialReligion::EasternOrthodox),
        "islamic" => Ok(OfficialReligion::Islamic),
        _ => Err(Error::InvalidField {
            path: path.to_path_buf(),
            field,
            value: value.into(),
            message: "unrecognized official religion".into(),
        }),
    }
}

fn parse_arrangement(
    path: &Path,
    religions: &str,
    church: &str,
) -> Result<WesternChristianArrangement> {
    match religions {
        "catholic_lutheran" => Ok(WesternChristianArrangement::CatholicLutheran {
            church: match church {
                "roman_catholic" => CatholicLutheranChurch::RomanCatholic,
                "lutheran" => CatholicLutheranChurch::Lutheran,
                _ => return invalid_arrangement(path, religions, church),
            },
        }),
        "catholic_reformed" => Ok(WesternChristianArrangement::CatholicReformed {
            church: match church {
                "roman_catholic" => CatholicReformedChurch::RomanCatholic,
                "reformed" => CatholicReformedChurch::Reformed,
                _ => return invalid_arrangement(path, religions, church),
            },
        }),
        "lutheran_reformed" => Ok(WesternChristianArrangement::LutheranReformed {
            church: match church {
                "lutheran" => LutheranReformedChurch::Lutheran,
                "reformed" => LutheranReformedChurch::Reformed,
                _ => return invalid_arrangement(path, religions, church),
            },
        }),
        _ => invalid_arrangement(path, religions, church),
    }
}

fn invalid_arrangement<T>(path: &Path, religions: &str, church: &str) -> Result<T> {
    Err(Error::InvalidField {
        path: path.to_path_buf(),
        field: "religions/church",
        value: format!("{religions}/{church}"),
        message: "church must belong to a supported two-confession arrangement".into(),
    })
}

fn read_regions(path: &Path) -> Result<Vec<Region>> {
    if !path.is_file() {
        return Err(Error::MissingSource(path.to_path_buf()));
    }
    let mut reader = csv::Reader::from_path(path).map_err(|source| Error::Csv {
        path: path.to_path_buf(),
        source,
    })?;
    let mut regions = Vec::new();
    for row in reader.deserialize() {
        let raw = row.map_err(|source| Error::Csv {
            path: path.to_path_buf(),
            source,
        })?;
        regions.push(Region::parse(path, raw)?);
    }
    if regions.is_empty() {
        return Err(Error::Validation(format!(
            "{} contains no IEG-derived regions",
            path.display()
        )));
    }
    let mut names = HashSet::with_capacity(regions.len());
    if let Some(duplicate) = regions
        .iter()
        .find(|region| !names.insert(region.name.as_str()))
    {
        return Err(Error::Validation(format!(
            "{} repeats IEG-derived region {:?}",
            path.display(),
            duplicate.name
        )));
    }
    let mut priorities = HashSet::with_capacity(regions.len());
    if let Some(duplicate) = regions
        .iter()
        .find(|region| !priorities.insert(region.priority))
    {
        return Err(Error::Validation(format!(
            "{} repeats IEG-derived priority {}",
            path.display(),
            duplicate.priority
        )));
    }
    regions.sort_by_key(|region| region.priority);
    Ok(regions)
}

pub(crate) fn enrich(
    mut draft: WorldDraft<GeologySettlementDraft>,
    regions_path: &Path,
) -> Result<WorldDraft<ReligionSettlementDraft>> {
    if draft.year != SUPPORTED_YEAR {
        return Err(Error::Validation(format!(
            "IEG intermediate represents {SUPPORTED_YEAR}, not {}",
            draft.year
        )));
    }
    let regions = read_regions(regions_path)?;

    let mut fallbacks = 0;
    let settlements: Vec<ReligionSettlementDraft> = std::mem::take(&mut draft.settlements)
        .into_iter()
        .map(|mut geologic| {
            let settlement = &geologic
                .predicted
                .trees
                .vegetated
                .forest
                .land
                .elevated
                .settlement;
            let matched = regions
                .iter()
                .find(|region| region.contains(settlement.latitude, settlement.longitude));
            let religious_status = matched.map(|region| region.status).unwrap_or_else(|| {
                    fallbacks += 1;
                    infer_fallback(settlement.latitude, settlement.longitude)
                });
            push_source_note(
                &mut geologic,
                if matched.is_some() {
                    "**[IEG AtlasEuropa religion maps](https://www.atlas-europa.de/t02/rel-anerkannt/t02-anerkannte-religionen.htm):** Official 1544 legal status comes from the prioritized, coarse project-curated intermediate between the 1500 and 1555 maps; it is a gameplay approximation rather than an exact historical border claim."
                } else {
                    "**IEG religion fallback:** No curated region covered the settlement, so official religion is deterministically assigned from the documented broad geographic fallback; personal belief is not inferred."
                },
            );
            ReligionSettlementDraft {
                geologic,
                religious_status,
            }
        })
        .collect();
    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.religion_regions_read = regions.len();
    draft.report.religion_samples = settlements.len();
    draft.report.religion_fallback_samples = fallbacks;
    Ok(WorldDraft {
        year: draft.year,
        spatial_grid: draft.spatial_grid,
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

fn infer_fallback(latitude: f64, longitude: f64) -> SettlementReligiousStatus {
    let religion = if longitude >= 27.0 && latitude <= 52.5 {
        OfficialReligion::EasternOrthodox
    } else {
        OfficialReligion::RomanCatholic
    };
    SettlementReligiousStatus::Established { religion }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(status: &str, religions: &str, church: &str) -> RawRegion {
        RawRegion {
            priority: 10,
            region: "test".into(),
            min_latitude: 50.0,
            max_latitude: 51.0,
            min_longitude: 10.0,
            max_longitude: 11.0,
            status: status.into(),
            religions: religions.into(),
            church: church.into(),
        }
    }

    #[test]
    fn parses_rows_directly_into_valid_legal_statuses() {
        let path = Path::new("regions.csv");
        assert_eq!(
            Region::parse(path, raw("established", "lutheran", ""))
                .unwrap()
                .status,
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::Lutheran
            }
        );
        assert_eq!(
            Region::parse(
                path,
                raw("multi_confessional", "catholic_lutheran", "roman_catholic")
            )
            .unwrap()
            .status
            .church(),
            OfficialReligion::RomanCatholic
        );
    }

    #[test]
    fn rejects_a_church_outside_its_legal_arrangement() {
        let error = Region::parse(
            Path::new("regions.csv"),
            raw("parity", "catholic_lutheran", "reformed"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("church must belong"));
    }

    #[test]
    fn rejects_zero_area_regions() {
        let mut region = raw("established", "lutheran", "");
        region.max_latitude = region.min_latitude;
        let error = Region::parse(Path::new("regions.csv"), region).unwrap_err();
        assert!(error.to_string().contains("positive width and height"));
    }

    #[test]
    fn fallback_is_complete_and_geographically_plausible() {
        assert_eq!(
            infer_fallback(50.0, 28.0).church(),
            OfficialReligion::EasternOrthodox
        );
        assert_eq!(
            infer_fallback(50.0, 8.0).church(),
            OfficialReligion::RomanCatholic
        );
    }

    #[test]
    fn checked_in_intermediate_parses_into_typed_regions() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/world-data/ieg-religion-1544.csv");
        let regions = read_regions(&path).unwrap();
        assert_eq!(regions.len(), 14);
        assert!(regions.iter().any(|region| {
            matches!(
                region.status,
                SettlementReligiousStatus::MultiConfessional { .. }
            )
        }));
        assert!(regions.iter().all(|region| !region.name.is_empty()));
        let upper_rhine = regions
            .iter()
            .find(|region| region.contains(50.5, 8.5))
            .unwrap();
        assert_eq!(upper_rhine.name, "Upper Rhine mixed territories");
        assert!(matches!(
            upper_rhine.status,
            SettlementReligiousStatus::MultiConfessional { .. }
        ));
    }
}
