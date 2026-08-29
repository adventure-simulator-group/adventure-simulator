use serde_json::Value;
use sha2::{Digest, Sha256};

const DIALOGUE_CATALOG_REVISION_DOMAIN: &[u8] = b"adventuresim.dialogue-catalog.v2\0";

pub(crate) fn catalog_revision(entries: &[(String, Value)]) -> String {
    let mut digest = Sha256::new();
    digest.update(DIALOGUE_CATALOG_REVISION_DOMAIN);
    digest.update((entries.len() as u64).to_le_bytes());
    for (path, document) in entries {
        update_bytes(&mut digest, path.as_bytes());
        update_json(&mut digest, document);
    }
    format!("{:x}", digest.finalize())
}

fn update_bytes(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}

fn update_json(digest: &mut Sha256, value: &Value) {
    match value {
        Value::Null => digest.update([0]),
        Value::Bool(value) => digest.update([1, u8::from(*value)]),
        Value::Number(value) => {
            digest.update([2]);
            update_bytes(digest, value.to_string().as_bytes());
        }
        Value::String(value) => {
            digest.update([3]);
            update_bytes(digest, value.as_bytes());
        }
        Value::Array(values) => {
            digest.update([4]);
            digest.update((values.len() as u64).to_le_bytes());
            for value in values {
                update_json(digest, value);
            }
        }
        Value::Object(values) => {
            digest.update([5]);
            digest.update((values.len() as u64).to_le_bytes());
            let mut fields: Vec<_> = values.iter().collect();
            fields.sort_unstable_by_key(|(name, _)| *name);
            for (name, value) in fields {
                update_bytes(digest, name.as_bytes());
                update_json(digest, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn revision_has_a_fixed_versioned_vector() {
        let entries = vec![(
            "content/dialogue/example.yaml".into(),
            json!({"conversation": {"roles": ["speaker", "listener"]}}),
        )];

        assert_eq!(
            catalog_revision(&entries),
            "278179d4c537a39319067a83919c051448b19142fca14a1082854880b0400d2d"
        );
    }

    #[test]
    fn object_field_order_does_not_change_the_revision() {
        let first: Value = serde_json::from_str(r#"{"first":1,"second":2}"#).unwrap();
        let second: Value = serde_json::from_str(r#"{ "second": 2, "first": 1 }"#).unwrap();

        assert_eq!(
            catalog_revision(&[("content/dialogue/example.yaml".into(), first)]),
            catalog_revision(&[("content/dialogue/example.yaml".into(), second)])
        );
    }
}
