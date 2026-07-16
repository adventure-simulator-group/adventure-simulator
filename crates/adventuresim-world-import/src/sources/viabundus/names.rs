use std::{collections::HashMap, path::Path};

use adventuresim_world_schema::{LanguageCode, SettlementAliasImport};
use serde::Deserialize;

use crate::{Error, Result};

use super::{active_in_year, optional_number, optional_text, read_csv, required_number};

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
        let from = optional_number(path, "year1", &raw.year1)?;
        let to = optional_number(path, "year2", &raw.year2)?;
        if !active_in_year(from, to, year) {
            continue;
        }
        let name = optional_text(&raw.name).ok_or_else(|| Error::InvalidField {
            path: path.into(),
            field: "name",
            value: raw.name.clone(),
            message: "value is required".into(),
        })?;
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
            prefix: optional_text(&raw.prefix),
            language,
        });
    }
    aliases.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(aliases)
}
