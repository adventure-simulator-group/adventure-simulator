use serde::Serialize;

const FORBIDDEN_FIELD_FRAGMENTS: &[&str] = &[
    "character_id",
    "relationship_id",
    "settlement_id",
    "difficulty_class",
    "raw_affinity",
    "father_affinity",
    "private_canary",
];

pub fn audit_json<T: Serialize>(value: &T) -> Result<Vec<String>, serde_json::Error> {
    let encoded = serde_json::to_string(value)?;
    let lower = encoded.to_ascii_lowercase();
    Ok(FORBIDDEN_FIELD_FRAGMENTS
        .iter()
        .filter(|fragment| lower.contains(**fragment))
        .map(|fragment| format!("forbidden report field fragment: {fragment}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn catches_private_authority_fields() {
        let findings = audit_json(&json!({"raw_affinity": 90})).unwrap();
        assert_eq!(findings.len(), 1);
        assert!(
            audit_json(&json!({"selected_role": "courtship_partner"}))
                .unwrap()
                .is_empty()
        );
    }
}
