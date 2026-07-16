use std::{collections::HashMap, path::Path};

use adventuresim_world_schema::{
    LanguageCode, SettlementDescriptionImport, SettlementDescriptionKind,
};
use serde::Deserialize;

use crate::{Error, Result};

use super::{optional_text, read_csv, required_number};

#[derive(Debug, Deserialize)]
struct RawDescription {
    id: String,
    nodesid: String,
    #[serde(default)]
    language: String,
    pertainsto: String,
    description: String,
}

pub(super) fn compile(
    path: &Path,
    settlement_ids: &HashMap<u64, String>,
) -> Result<Vec<SettlementDescriptionImport>> {
    let mut descriptions = Vec::new();
    for raw in read_csv::<RawDescription>(path)? {
        let kind = match raw.pertainsto.trim() {
            "settlement" => SettlementDescriptionKind::Settlement,
            "city" => SettlementDescriptionKind::City,
            _ => continue,
        };
        let source_id: u64 = required_number(path, "id", &raw.id)?;
        let node_id: u64 = required_number(path, "nodesid", &raw.nodesid)?;
        let Some(settlement_id) = settlement_ids.get(&node_id) else {
            continue;
        };
        let language = optional_text(&raw.language)
            .map(|value| value.parse::<LanguageCode>())
            .transpose()
            .map_err(|error| Error::InvalidField {
                path: path.into(),
                field: "language",
                value: raw.language.clone(),
                message: error.to_string(),
            })?;
        let body = normalize_description(&raw.description);
        if body.is_empty() {
            return Err(Error::InvalidField {
                path: path.into(),
                field: "description",
                value: raw.description,
                message: "description is empty after normalizing source markup".into(),
            });
        }
        descriptions.push(SettlementDescriptionImport {
            id: format!("viabundus-description-{source_id}"),
            settlement_id: settlement_id.clone(),
            kind,
            language,
            body,
        });
    }
    descriptions.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(descriptions)
}

fn normalize_description(source: &str) -> String {
    let decoded = html_escape::decode_html_entities(source);
    let mut plain = String::with_capacity(decoded.len());
    let mut in_tag = false;
    for character in decoded.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                plain.push(' ');
            }
            _ if !in_tag => plain.push(character),
            _ => {}
        }
    }
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::normalize_description;

    #[test]
    fn descriptions_become_plain_text_at_the_source_boundary() {
        assert_eq!(
            normalize_description("&lt;b&gt;Stadt:&lt;/b&gt; Burg &amp; Markt."),
            "Stadt: Burg & Markt."
        );
    }
}
