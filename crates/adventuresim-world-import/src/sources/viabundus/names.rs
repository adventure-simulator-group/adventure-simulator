use std::{collections::HashMap, path::Path};

use adventuresim_world_schema::{
    LanguageCode, SETTLEMENT_ALIAS_NAME_MAX_BYTES, SETTLEMENT_ALIAS_PREFIX_MAX_BYTES,
    SettlementAliasImport, valid_bounded_source_text,
};
use serde::Deserialize;

use crate::{Error, Result};

use super::{ActiveInterval, optional_text, read_csv, required_number};

#[derive(Debug, Deserialize)]
struct RawAlternativeName {
    id: String,
    name: String,
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    year1: String,
    #[serde(default)]
    year2: String,
    #[serde(default)]
    language: String,
    nodesid: String,
}

pub(super) fn compile(
    path: &Path,
    year: i32,
    settlement_ids: &HashMap<u64, String>,
) -> Result<Vec<SettlementAliasImport>> {
    let mut aliases = Vec::new();
    for raw in read_csv::<RawAlternativeName>(path)? {
        let source_id: u64 = required_number(path, "id", &raw.id)?;
        let node_id: u64 = required_number(path, "nodesid", &raw.nodesid)?;
        let Some(settlement_id) = settlement_ids.get(&node_id) else {
            continue;
        };
        let interval = ActiveInterval::parse(path, "year1", &raw.year1, "year2", &raw.year2)?;
        if !interval.contains(year) {
            continue;
        }
        let name = optional_text(&raw.name).ok_or_else(|| Error::InvalidField {
            path: path.into(),
            field: "name",
            value: raw.name.clone(),
            message: "value is required".into(),
        })?;
        require_bounded(
            path,
            "name",
            source_id,
            &name,
            SETTLEMENT_ALIAS_NAME_MAX_BYTES,
        )?;
        let prefix = optional_text(&raw.prefix);
        if let Some(value) = &prefix {
            require_bounded(
                path,
                "prefix",
                source_id,
                value,
                SETTLEMENT_ALIAS_PREFIX_MAX_BYTES,
            )?;
        }
        let language = optional_text(&raw.language)
            .map(|value| value.parse::<LanguageCode>())
            .transpose()
            .map_err(|error| Error::InvalidField {
                path: path.into(),
                field: "language",
                value: raw.language.clone(),
                message: error.to_string(),
            })?;
        aliases.push(SettlementAliasImport {
            id: format!("viabundus-alias-{source_id}"),
            settlement_id: settlement_id.clone(),
            name,
            prefix,
            language,
        });
    }
    aliases.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(aliases)
}

fn require_bounded(
    path: &Path,
    field: &'static str,
    source_id: u64,
    value: &str,
    max_bytes: usize,
) -> Result<()> {
    if valid_bounded_source_text(value, max_bytes) {
        Ok(())
    } else {
        Err(Error::InvalidField {
            path: path.into(),
            field,
            value: format!("Viabundus record {source_id}"),
            message: format!("must be trimmed, NUL-free, and at most {max_bytes} bytes"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_source_fields_enforce_limits_and_nul_rejection() {
        let path = Path::new("alternativenames.csv");
        let limit = SETTLEMENT_ALIAS_NAME_MAX_BYTES;
        assert!(require_bounded(path, "name", 7, &"a".repeat(limit), limit).is_ok());
        let oversized = require_bounded(path, "name", 7, &"a".repeat(limit + 1), limit)
            .unwrap_err()
            .to_string();
        assert!(oversized.contains("name"));
        assert!(oversized.contains("record 7"));
        assert!(require_bounded(path, "name", 8, "visible\0hidden", limit).is_err());
    }
}
