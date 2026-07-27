//! Dependency-light validation shared by the item build compiler and tests.

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub fn parse_document(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str::<StrictValue>(text).map(|value| value.0)
}

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

struct StrictValueVisitor;

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a strict JSON-compatible YAML value")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue(value.into()))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictValue(value.into()))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictValue(value.into()))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .map(StrictValue)
            .ok_or_else(|| E::custom("non-finite number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(StrictValue(value.into()))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictValue(value.into()))
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.visit_none()
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictValue>()? {
            values.push(value.0);
        }
        Ok(StrictValue(values.into()))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut values = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom(format!("duplicate mapping key {key:?}")));
            }
            values.insert(key, map.next_value::<StrictValue>()?.0);
        }
        Ok(StrictValue(
            values.into_iter().collect::<Map<_, _>>().into(),
        ))
    }
}

pub fn validate_documents(documents: &[Value], files: &[String]) -> Result<(), String> {
    let mut errors = Vec::new();
    let mut ids = BTreeMap::<String, String>::new();
    for (document, file) in documents.iter().zip(files) {
        let Some(root) = document.as_object() else {
            errors.push(format!("{file}: catalog: root must be an object"));
            continue;
        };
        reject_unknown(
            root,
            &["schema_version", "items"],
            file,
            "catalog",
            &mut errors,
        );
        if root.get("schema_version").and_then(Value::as_u64) != Some(1) {
            errors.push(format!(
                "{file}: catalog.schema_version: supported value is 1"
            ));
        }
        let Some(items) = root.get("items").and_then(Value::as_array) else {
            errors.push(format!("{file}: catalog.items: required array"));
            continue;
        };
        for (index, item) in items.iter().enumerate() {
            validate_item(item, file, index, &mut ids, &mut errors);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "item catalog validation failed ({} errors):\n{}",
            errors.len(),
            errors.join("\n")
        ))
    }
}

fn validate_item(
    value: &Value,
    file: &str,
    index: usize,
    ids: &mut BTreeMap<String, String>,
    errors: &mut Vec<String>,
) {
    let Some(item) = value.as_object() else {
        errors.push(format!("{file}: items.{index}: item must be an object"));
        return;
    };
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("<missing>");
    let path = format!("item {id}");
    reject_unknown(
        item,
        &[
            "id",
            "display_name",
            "weight_kg",
            "base_value",
            "tags",
            "presentation",
            "kind",
            "slot",
            "block",
            "coverage",
            "resistance",
            "padding",
            "flexibility",
            "range_of_motion",
            "accuracy",
            "reach_m",
            "penetration",
            "balance",
            "precise",
            "melee",
            "ranged",
            "damage_types",
            "skills",
            "capabilities",
        ],
        file,
        &path,
        errors,
    );
    if !valid_id(id) {
        errors.push(format!(
            "{file}: {path}.id: must be 1..64 lowercase ASCII letters, digits, or underscores"
        ));
    } else if let Some(first) = ids.insert(id.to_owned(), file.to_owned()) {
        errors.push(format!(
            "{file}: {path}.id: duplicate stable ID (first defined in {first})"
        ));
    }
    required_string(item, "display_name", file, &path, errors);
    finite_in(item, "weight_kg", 0.0, 10_000.0, file, &path, errors);
    if item.get("base_value").and_then(Value::as_u64).is_none() {
        errors.push(format!(
            "{file}: {path}.base_value: required non-negative integer"
        ));
    }
    match item.get("presentation").and_then(Value::as_object) {
        Some(presentation) => {
            reject_unknown(
                presentation,
                &["icon"],
                file,
                &format!("{path}.presentation"),
                errors,
            );
            required_string(
                presentation,
                "icon",
                file,
                &format!("{path}.presentation"),
                errors,
            );
        }
        None => errors.push(format!("{file}: {path}.presentation: required object")),
    }
    let kind = item.get("kind").and_then(Value::as_str).unwrap_or("");
    let supported: BTreeSet<_> = [
        "simple",
        "currency",
        "ingredient",
        "medication",
        "clothing",
        "container",
        "shield",
        "armor",
        "weapon",
        "food",
    ]
    .into_iter()
    .collect();
    if !supported.contains(kind) {
        errors.push(format!("{file}: {path}.kind: unsupported kind {kind:?}"));
    }
    let slot = item.get("slot").and_then(Value::as_str).unwrap_or("none");
    if kind == "weapon" || kind == "shield" {
        if slot != "any_holding" {
            errors.push(format!("{file}: {path}.slot: {kind} requires any_holding"));
        }
    } else if kind == "armor"
        && !matches!(slot, "head" | "chest" | "stomach" | "any_arm" | "any_leg")
    {
        errors.push(format!("{file}: {path}.slot: invalid armor slot"));
    }
    if kind == "weapon" {
        validate_weapon(item, file, &path, errors);
    } else {
        for field in [
            "accuracy",
            "reach_m",
            "penetration",
            "balance",
            "precise",
            "melee",
            "ranged",
            "damage_types",
            "skills",
        ] {
            if item.contains_key(field) {
                errors.push(format!(
                    "{file}: {path}.{field}: field is only valid for weapon"
                ));
            }
        }
    }
    if kind != "armor" {
        for field in [
            "coverage",
            "resistance",
            "padding",
            "flexibility",
            "range_of_motion",
        ] {
            if item.contains_key(field) {
                errors.push(format!(
                    "{file}: {path}.{field}: field is only valid for armor"
                ));
            }
        }
    }
    if kind != "shield" && item.contains_key("block") {
        errors.push(format!(
            "{file}: {path}.block: field is only valid for shield"
        ));
    }
    if !matches!(kind, "weapon" | "armor" | "shield" | "container") && item.contains_key("slot") {
        errors.push(format!("{file}: {path}.slot: kind does not accept a slot"));
    }
    if let Some(capabilities) = item.get("capabilities").and_then(Value::as_object) {
        validate_capabilities(capabilities, file, &path, kind, errors);
    }
}

fn validate_weapon(item: &Map<String, Value>, file: &str, path: &str, errors: &mut Vec<String>) {
    for field in ["accuracy", "reach_m", "penetration", "balance"] {
        finite_in(item, field, 0.0, 10_000.0, file, path, errors);
    }
    let melee = item.get("melee").and_then(Value::as_bool).unwrap_or(false);
    let ranged = item.get("ranged").and_then(Value::as_bool).unwrap_or(false);
    if !melee && !ranged {
        errors.push(format!(
            "{file}: {path}: weapon must be melee and/or ranged"
        ));
    }
    let damage = item.get("damage_types").and_then(Value::as_array);
    if damage.is_none_or(Vec::is_empty)
        || damage.is_some_and(|values| {
            values
                .iter()
                .any(|value| !matches!(value.as_str(), Some("blunt" | "slash" | "pierce")))
        })
    {
        errors.push(format!(
            "{file}: {path}.damage_types: explicit non-empty supported damage types required"
        ));
    }
    let Some(skills) = item.get("skills").and_then(Value::as_object) else {
        errors.push(format!(
            "{file}: {path}.skills: explicit normalized distribution required"
        ));
        return;
    };
    reject_unknown(
        skills,
        &[
            "polearm", "axe", "bludgeon", "sword", "knife", "bow", "crossbow", "firearm", "throw",
        ],
        file,
        &format!("{path}.skills"),
        errors,
    );
    let total: f64 = [
        "polearm", "axe", "bludgeon", "sword", "knife", "bow", "crossbow", "firearm", "throw",
    ]
    .into_iter()
    .filter_map(|key| skills.get(key).and_then(Value::as_f64))
    .sum();
    if skills
        .values()
        .any(|v| v.as_f64().is_none_or(|n| !n.is_finite() || n < 0.0))
        || (total - 1.0).abs() > 0.000_1
    {
        errors.push(format!(
            "{file}: {path}.skills: weights must be finite, non-negative, and sum to 1"
        ));
    }
    let melee_total: f64 = ["polearm", "axe", "bludgeon", "sword", "knife"]
        .into_iter()
        .filter_map(|key| skills.get(key).and_then(Value::as_f64))
        .sum();
    let ranged_total: f64 = ["bow", "crossbow", "firearm", "throw"]
        .into_iter()
        .filter_map(|key| skills.get(key).and_then(Value::as_f64))
        .sum();
    if (melee && melee_total <= 0.0) || (ranged && ranged_total <= 0.0) {
        errors.push(format!(
            "{file}: {path}.skills: distribution is incompatible with weapon mode"
        ));
    }
}

fn validate_capabilities(
    capabilities: &Map<String, Value>,
    file: &str,
    path: &str,
    kind: &str,
    errors: &mut Vec<String>,
) {
    reject_unknown(
        capabilities,
        &["durability", "food", "alcohol", "container"],
        file,
        &format!("{path}.capabilities"),
        errors,
    );
    if let Some(durability) = capabilities.get("durability").and_then(Value::as_object) {
        reject_unknown(
            durability,
            &[
                "quality",
                "yield_j",
                "fracture_j",
                "wear",
                "failure_share",
                "edge_sensitivity",
                "handling_sensitivity",
            ],
            file,
            &format!("{path}.capabilities.durability"),
            errors,
        );
        let quality = durability
            .get("quality")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if !(1..=5).contains(&quality) {
            errors.push(format!(
                "{file}: {path}.capabilities.durability.quality: expected 1..5"
            ));
        }
        for field in ["yield_j", "fracture_j"] {
            finite_in(durability, field, 0.0, 1_000_000.0, file, path, errors);
        }
        for field in [
            "wear",
            "failure_share",
            "edge_sensitivity",
            "handling_sensitivity",
        ] {
            finite_in(durability, field, 0.0, 1.0, file, path, errors);
        }
        let yield_j = durability
            .get("yield_j")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let fracture_j = durability
            .get("fracture_j")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if yield_j <= 0.0 || fracture_j < yield_j {
            errors.push(format!(
                "{file}: {path}.capabilities.durability: yield must be positive and fracture must be at least yield"
            ));
        }
    } else if matches!(kind, "weapon" | "armor" | "shield" | "clothing") {
        errors.push(format!(
            "{file}: {path}.capabilities.durability: required for repairable kind"
        ));
    }
    if let Some(food) = capabilities.get("food").and_then(Value::as_object) {
        reject_unknown(
            food,
            &[
                "class",
                "nutrition_kcal",
                "value_per_unit",
                "growth_per_hour",
                "cooking_minutes",
                "flavors_kg",
                "culinary_fat",
                "quality",
            ],
            file,
            &format!("{path}.capabilities.food"),
            errors,
        );
        if !matches!(
            food.get("class").and_then(Value::as_str),
            Some(
                "ration"
                    | "grain"
                    | "bread"
                    | "fruit"
                    | "berries"
                    | "vegetable"
                    | "nuts"
                    | "herb"
                    | "mushroom"
                    | "raw_meat"
                    | "cooked_meat"
                    | "mixed_meal"
            )
        ) {
            errors.push(format!(
                "{file}: {path}.capabilities.food.class: unsupported food class"
            ));
        }
        for field in ["nutrition_kcal", "value_per_unit", "growth_per_hour"] {
            finite_in(food, field, 0.0, 1_000_000.0, file, path, errors);
        }
        let quality = food.get("quality").and_then(Value::as_u64).unwrap_or(0);
        if !(1..=5).contains(&quality) {
            errors.push(format!(
                "{file}: {path}.capabilities.food.quality: expected 1..5"
            ));
        }
        match food.get("flavors_kg").and_then(Value::as_object) {
            Some(flavors) => {
                reject_unknown(
                    flavors,
                    &["salty", "spicy", "sweet", "sour", "savory"],
                    file,
                    &format!("{path}.capabilities.food.flavors_kg"),
                    errors,
                );
                for field in ["salty", "spicy", "sweet", "sour", "savory"] {
                    finite_in(flavors, field, 0.0, 1_000.0, file, path, errors);
                }
            }
            None => errors.push(format!(
                "{file}: {path}.capabilities.food.flavors_kg: required object"
            )),
        }
    } else if kind == "food" {
        errors.push(format!(
            "{file}: {path}.capabilities.food: food kind requires nutrition metadata"
        ));
    }
    if let Some(alcohol) = capabilities.get("alcohol").and_then(Value::as_object) {
        reject_unknown(
            alcohol,
            &[
                "serving_ml",
                "abv_basis_points",
                "net_hydration_ml",
                "disinfectant_effectiveness",
                "disinfectant_focused",
                "potable",
            ],
            file,
            &format!("{path}.capabilities.alcohol"),
            errors,
        );
        let serving = alcohol
            .get("serving_ml")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let abv = alcohol
            .get("abv_basis_points")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let hydration = alcohol
            .get("net_hydration_ml")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        if serving == 0 || serving > 10_000 || abv > 10_000 || hydration > serving {
            errors.push(format!(
                "{file}: {path}.capabilities.alcohol: invalid ml/ABV values"
            ));
        }
        if alcohol.get("potable").and_then(Value::as_bool).is_none()
            || alcohol
                .get("disinfectant_focused")
                .and_then(Value::as_bool)
                .is_none()
            || alcohol
                .get("disinfectant_effectiveness")
                .and_then(Value::as_u64)
                .is_none_or(|value| value > u16::MAX.into())
        {
            errors.push(format!(
                "{file}: {path}.capabilities.alcohol: missing or invalid behavior metadata"
            ));
        }
    }
    if let Some(container) = capabilities.get("container").and_then(Value::as_object) {
        let capacity = container
            .get("capacity_ml")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if capacity == 0 || capacity > 1_000_000 {
            errors.push(format!(
                "{file}: {path}.capabilities.container.capacity_ml: expected 1..1000000"
            ));
        }
    }
    if kind == "container" && !capabilities.contains_key("container") {
        errors.push(format!(
            "{file}: {path}.capabilities.container: container kind requires capacity metadata"
        ));
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn required_string(
    object: &Map<String, Value>,
    field: &str,
    file: &str,
    path: &str,
    errors: &mut Vec<String>,
) {
    if object
        .get(field)
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        errors.push(format!("{file}: {path}.{field}: required non-empty string"));
    }
}

fn finite_in(
    object: &Map<String, Value>,
    field: &str,
    min: f64,
    max: f64,
    file: &str,
    path: &str,
    errors: &mut Vec<String>,
) {
    if object
        .get(field)
        .and_then(Value::as_f64)
        .is_none_or(|value| !value.is_finite() || value < min || value > max)
    {
        errors.push(format!(
            "{file}: {path}.{field}: expected finite value in {min}..={max}"
        ));
    }
}

fn reject_unknown(
    object: &Map<String, Value>,
    allowed: &[&str],
    file: &str,
    path: &str,
    errors: &mut Vec<String>,
) {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            errors.push(format!("{file}: {path}.{key}: unknown field"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_item(id: &str) -> Value {
        json!({
            "id": id,
            "display_name": "Test",
            "weight_kg": 1.0,
            "base_value": 1,
            "tags": [],
            "presentation": {"icon": "help"},
            "kind": "simple"
        })
    }

    fn document(items: Vec<Value>) -> Value {
        json!({"schema_version": 1, "items": items})
    }

    #[test]
    fn malformed_documents_aggregate_schema_unknown_and_duplicate_id_errors() {
        let mut bad = valid_item("same");
        bad.as_object_mut()
            .unwrap()
            .insert("mystery".into(), json!(true));
        let documents = vec![
            json!({"schema_version": 99, "items": [bad]}),
            document(vec![valid_item("same")]),
        ];
        let error = validate_documents(
            &documents,
            &["content/items/a.yaml".into(), "content/items/b.yaml".into()],
        )
        .unwrap_err();
        assert!(error.contains("schema_version"));
        assert!(error.contains("unknown field"));
        assert!(error.contains("duplicate stable ID"));
        assert!(error.contains("content/items/a.yaml"));
        assert!(error.contains("content/items/b.yaml"));
    }

    #[test]
    fn duplicate_mapping_keys_report_key_line_and_column() {
        let source = "{\n  \"schema_version\": 1,\n  \"schema_version\": 1,\n  \"items\": []\n}";
        let error = parse_document(source).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate mapping key \"schema_version\"")
        );
        assert_eq!(error.line(), 3);
        assert!(error.column() > 0);
    }

    #[test]
    fn invalid_numbers_kinds_and_weapon_distributions_are_rejected() {
        let mut weapon = valid_item("BAD-ID");
        weapon.as_object_mut().unwrap().extend([
            ("weight_kg".into(), json!(-1)),
            ("kind".into(), json!("weapon")),
            ("slot".into(), json!("head")),
            ("accuracy".into(), json!(1)),
            ("reach_m".into(), json!(1)),
            ("penetration".into(), json!(1)),
            ("balance".into(), json!(1)),
            ("precise".into(), json!(false)),
            ("melee".into(), json!(true)),
            ("ranged".into(), json!(false)),
            ("damage_types".into(), json!([])),
            ("skills".into(), json!({"laser": 1.0})),
        ]);
        let error =
            validate_documents(&[document(vec![weapon])], &["fixture.yaml".into()]).unwrap_err();
        for expected in [
            ".id:",
            "weight_kg",
            "requires any_holding",
            "damage_types",
            "unknown field",
            "sum to 1",
            "incompatible",
        ] {
            assert!(error.contains(expected), "{error}");
        }
    }
}
