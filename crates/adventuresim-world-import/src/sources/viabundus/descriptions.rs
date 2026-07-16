use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use adventuresim_world_schema::{
    LanguageCode, SETTLEMENT_DESCRIPTION_MAX_BYTES, SettlementDescriptionImport,
    SettlementDescriptionKind, valid_bounded_source_text,
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
    pertainsto: DescriptionSubject,
    description: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DescriptionSubject {
    Bridge,
    City,
    Fair,
    Ferry,
    Harbour,
    Lock,
    Settlement,
    Staple,
    Toll,
}

impl DescriptionSubject {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Bridge => "bridge",
            Self::City => "city",
            Self::Fair => "fair",
            Self::Ferry => "ferry",
            Self::Harbour => "harbour",
            Self::Lock => "lock",
            Self::Settlement => "settlement",
            Self::Staple => "staple",
            Self::Toll => "toll",
        }
    }
}

pub(super) fn compile(
    path: &Path,
    settlement_ids: &HashMap<u64, String>,
) -> Result<(Vec<SettlementDescriptionImport>, BTreeMap<String, usize>)> {
    let mut descriptions = Vec::new();
    let mut deferred = BTreeMap::new();
    for raw in read_csv::<RawDescription>(path)? {
        let kind = match raw.pertainsto {
            DescriptionSubject::Settlement => SettlementDescriptionKind::Settlement,
            DescriptionSubject::City => SettlementDescriptionKind::City,
            subject => {
                *deferred.entry(subject.as_str().to_owned()).or_default() += 1;
                continue;
            }
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
        require_bounded_description(path, source_id, &body)?;
        descriptions.push(SettlementDescriptionImport {
            id: format!("viabundus-description-{source_id}"),
            settlement_id: settlement_id.clone(),
            kind,
            language,
            body,
        });
    }
    descriptions.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((descriptions, deferred))
}

fn require_bounded_description(path: &Path, source_id: u64, body: &str) -> Result<()> {
    if valid_bounded_source_text(body, SETTLEMENT_DESCRIPTION_MAX_BYTES) {
        Ok(())
    } else {
        Err(Error::InvalidField {
            path: path.into(),
            field: "description",
            value: format!("Viabundus record {source_id}"),
            message: format!(
                "must be trimmed, NUL-free, and at most {SETTLEMENT_DESCRIPTION_MAX_BYTES} bytes"
            ),
        })
    }
}

fn normalize_description(source: &str) -> String {
    let decoded = html_escape::decode_html_entities(source);
    let mut plain = String::with_capacity(decoded.len());
    let mut remaining = decoded.as_ref();
    while let Some(start) = remaining.find('<') {
        plain.push_str(&remaining[..start]);
        let candidate = &remaining[start + 1..];
        let Some(end) = candidate.find('>') else {
            plain.push_str(&remaining[start..]);
            remaining = "";
            break;
        };
        let tag = &candidate[..end];
        if is_source_tag(tag) {
            let after_tag = &candidate[end + 1..];
            let separates_words = plain
                .chars()
                .next_back()
                .is_some_and(|character| !character.is_whitespace())
                && after_tag.chars().next().is_some_and(|character| {
                    !character.is_whitespace()
                        && character != '<'
                        && !character.is_ascii_punctuation()
                });
            if separates_words {
                plain.push(' ');
            }
            remaining = after_tag;
        } else {
            plain.push('<');
            remaining = candidate;
        }
    }
    plain.push_str(remaining);
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_source_tag(candidate: &str) -> bool {
    let candidate = candidate
        .strip_prefix('/')
        .or_else(|| candidate.strip_prefix('!'))
        .unwrap_or(candidate);
    candidate
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
        && candidate.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    ' ' | '\t' | '\r' | '\n' | '/' | '=' | '"' | '\'' | '-' | '_' | ':' | '.'
                )
        })
}

#[cfg(test)]
mod tests {
    use super::{normalize_description, require_bounded_description};
    use adventuresim_world_schema::SETTLEMENT_DESCRIPTION_MAX_BYTES;
    use std::path::Path;

    #[test]
    fn descriptions_become_plain_text_at_the_source_boundary() {
        assert_eq!(
            normalize_description("&lt;b&gt;Stadt:&lt;/b&gt; Burg &amp; Markt."),
            "Stadt: Burg & Markt."
        );
        assert_eq!(
            normalize_description("&lt;p&gt;Nested &lt;strong&gt;text&lt;/strong&gt;.&lt;/p&gt;"),
            "Nested text."
        );
        assert_eq!(
            normalize_description("population &lt; 1000"),
            "population < 1000"
        );
        assert_eq!(
            normalize_description("unfinished <strong"),
            "unfinished <strong"
        );
        assert_eq!(
            normalize_description("value &lt; strong&gt; than before"),
            "value < strong> than before"
        );
    }

    #[test]
    fn description_source_fields_enforce_limits_and_nul_rejection() {
        let path = Path::new("descriptions.csv");
        let limit = SETTLEMENT_DESCRIPTION_MAX_BYTES;
        assert!(require_bounded_description(path, 9, &"a".repeat(limit)).is_ok());
        let oversized = require_bounded_description(path, 9, &"a".repeat(limit + 1))
            .unwrap_err()
            .to_string();
        assert!(oversized.contains("description"));
        assert!(oversized.contains("record 9"));
        assert!(require_bounded_description(path, 10, "visible\0hidden").is_err());
    }
}
