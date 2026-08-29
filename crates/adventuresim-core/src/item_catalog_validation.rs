//! Dependency-light validation shared by the item build compiler and tests.

use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::{error::Error, fmt};

const MAX_DOCUMENTS: usize = 32;
const MAX_ITEMS: usize = 4_096;
const MAX_STRING_BYTES: usize = 256;
const MAX_TAGS: usize = 32;
const MAX_DIAGNOSTICS: usize = 128;
pub const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogDiagnostic {
    pub source_index: usize,
    pub path: String,
    pub message: String,
}

impl CatalogDiagnostic {
    fn new(source_index: usize, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            source_index,
            path: path.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogValidationError {
    diagnostics: Vec<CatalogDiagnostic>,
    files: Vec<String>,
    sources: Vec<String>,
}

impl CatalogValidationError {
    pub fn diagnostics(&self) -> &[CatalogDiagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for CatalogValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "item catalog validation failed ({} errors):",
            self.diagnostics.len()
        )?;
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index > 0 {
                writeln!(formatter)?;
            }
            let file = self
                .files
                .get(diagnostic.source_index)
                .map(String::as_str)
                .unwrap_or("item catalog");
            if let Some(source) = self.sources.get(diagnostic.source_index) {
                let (line, column) = source_coordinates(source, &diagnostic.path);
                write!(
                    formatter,
                    "{file}:{line}:{column}: {}: {}",
                    diagnostic.path, diagnostic.message
                )?;
            } else {
                write!(
                    formatter,
                    "{file}: {}: {}",
                    diagnostic.path, diagnostic.message
                )?;
            }
        }
        Ok(())
    }
}

impl Error for CatalogValidationError {}

struct CatalogDiagnostics<'a> {
    source_index: usize,
    diagnostics: &'a mut Vec<CatalogDiagnostic>,
}

impl CatalogDiagnostics<'_> {
    fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.diagnostics
            .push(CatalogDiagnostic::new(self.source_index, path, message));
    }
}

fn source_coordinates(source: &str, path: &str) -> (usize, usize) {
    let item_id = path
        .strip_prefix("item ")
        .and_then(|path| path.split(['.', ':']).next());
    let item_start = item_id
        .and_then(|id| source.find(&format!("\"id\": \"{id}\"")))
        .unwrap_or(0);
    let field = path
        .split('.')
        .rev()
        .find(|component| !component.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|component| component.split_whitespace().last())
        .unwrap_or("items");
    let offset = source[item_start..]
        .find(&format!("\"{field}\""))
        .map(|offset| item_start + offset)
        .unwrap_or(item_start);
    let before = &source[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(1, |line| line.chars().count() + 1);
    (line, column)
}

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

pub fn validate_documents(
    documents: &[Value],
    files: &[String],
) -> Result<(), CatalogValidationError> {
    validate_documents_with_sources(documents, files, &[])
}

pub fn validate_documents_with_sources(
    documents: &[Value],
    files: &[String],
    sources: &[String],
) -> Result<(), CatalogValidationError> {
    let mut diagnostics = Vec::new();
    let mut ids = BTreeMap::<String, usize>::new();
    if documents.len() > MAX_DOCUMENTS {
        diagnostics.push(CatalogDiagnostic::new(
            0,
            "item catalog",
            format!("at most {MAX_DOCUMENTS} source documents are supported"),
        ));
    }
    for (source_index, source) in sources.iter().enumerate() {
        if source.len() > MAX_SOURCE_BYTES {
            diagnostics.push(CatalogDiagnostic::new(
                source_index,
                "item catalog",
                format!("source exceeds {MAX_SOURCE_BYTES} bytes"),
            ));
        }
    }
    let item_count: usize = documents
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|root| root.get("items"))
        .filter_map(Value::as_array)
        .map(Vec::len)
        .sum();
    if item_count > MAX_ITEMS {
        diagnostics.push(CatalogDiagnostic::new(
            0,
            "item catalog",
            format!("at most {MAX_ITEMS} items are supported"),
        ));
    }
    for (source_index, document) in documents.iter().enumerate() {
        let mut errors = CatalogDiagnostics {
            source_index,
            diagnostics: &mut diagnostics,
        };
        let Some(root) = document.as_object() else {
            errors.push("catalog", "root must be an object");
            continue;
        };
        reject_unknown(root, &["schema_version", "items"], "catalog", &mut errors);
        if root.get("schema_version").and_then(Value::as_u64) != Some(1) {
            errors.push("catalog.schema_version", "supported value is 1");
        }
        let Some(items) = root.get("items").and_then(Value::as_array) else {
            errors.push("catalog.items", "required array");
            continue;
        };
        for (index, item) in items.iter().enumerate() {
            validate_item(item, index, &mut ids, &mut errors);
        }
        if let Err(error) = serde_json::from_value::<crate::item_catalog_schema::ItemCatalogDocument>(
            document.clone(),
        ) {
            errors.push("item catalog", format!("typed schema mismatch: {error}"));
        }
    }
    for (kind, expected) in [
        ("currency", crate::item_references::CURRENCY_IDS.as_slice()),
        (
            "medication",
            crate::item_references::MEDICATION_IDS.as_slice(),
        ),
    ] {
        let authored = documents
            .iter()
            .filter_map(|document| document.get("items"))
            .filter_map(Value::as_array)
            .flatten()
            .filter(|item| item.get("kind").and_then(Value::as_str) == Some(kind))
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        let expected = expected.iter().copied().collect::<BTreeSet<_>>();
        if authored != expected {
            diagnostics.push(CatalogDiagnostic::new(
                0,
                "item catalog",
                format!(
                    "{kind} IDs must match the supported gameplay registry; authored={authored:?}, expected={expected:?}"
                ),
            ));
        }
    }
    diagnostics.truncate(MAX_DIAGNOSTICS);
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(CatalogValidationError {
            diagnostics,
            files: files.to_vec(),
            sources: sources.to_vec(),
        })
    }
}

fn validate_item(
    value: &Value,
    index: usize,
    ids: &mut BTreeMap<String, usize>,
    errors: &mut CatalogDiagnostics<'_>,
) {
    let Some(item) = value.as_object() else {
        errors.push(format!("items.{index}"), "item must be an object");
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
            "exterior_volume_ml",
            "base_value",
            "tags",
            "presentation",
            "equipment",
            "kind",
            "slot",
            "carry",
            "handling",
            "animation_pack",
            "block",
            "coverage",
            "resistance",
            "padding",
            "flexibility",
            "range_of_motion",
            "accuracy",
            "preferred_attack",
            "swing_precision",
            "stab_precision",
            "reach_m",
            "penetration",
            "moment_of_inertia_kg_m2",
            "precise",
            "melee",
            "ranged",
            "damage_types",
            "skills",
            "capabilities",
        ],
        &path,
        errors,
    );
    if !valid_id(id) {
        errors.push(
            format!("{path}.id"),
            "must be 1..64 lowercase ASCII letters, digits, or underscores",
        );
    } else if let Some(first) = ids.insert(id.to_owned(), errors.source_index) {
        errors.push(
            format!("{path}.id"),
            format!("duplicate stable ID (first defined in source {first})"),
        );
    }
    required_string(item, "display_name", &path, errors);
    if item
        .get("display_name")
        .and_then(Value::as_str)
        .is_some_and(|value| value.len() > MAX_STRING_BYTES)
    {
        errors.push(
            format!("{path}.display_name"),
            format!("exceeds {MAX_STRING_BYTES} bytes"),
        );
    }
    match item.get("tags").and_then(Value::as_array) {
        Some(tags) if tags.len() <= MAX_TAGS => {
            let mut unique = BTreeSet::new();
            for tag in tags {
                match tag.as_str() {
                    Some(tag) if valid_id(tag) && unique.insert(tag) => {}
                    Some(tag) if !valid_id(tag) => {
                        errors.push(format!("{path}.tags"), format!("invalid tag {tag:?}"));
                    }
                    Some(tag) => {
                        errors.push(format!("{path}.tags"), format!("duplicate tag {tag:?}"));
                    }
                    None => errors.push(format!("{path}.tags"), "tags must be strings"),
                }
            }
        }
        Some(_) => errors.push(format!("{path}.tags"), format!("at most {MAX_TAGS} tags")),
        None => errors.push(format!("{path}.tags"), "required array"),
    }
    finite_in(item, "weight_kg", 0.0, 10_000.0, &path, errors);
    if !item
        .get("exterior_volume_ml")
        .and_then(Value::as_u64)
        .is_some_and(|volume| (1..=1_000_000).contains(&volume))
    {
        errors.push(format!("{path}.exterior_volume_ml"), "expected 1..1000000");
    }
    if item.get("base_value").and_then(Value::as_u64).is_none() {
        errors.push(
            format!("{path}.base_value"),
            "required non-negative integer",
        );
    }
    match item.get("presentation").and_then(Value::as_object) {
        Some(presentation) => {
            reject_unknown(
                presentation,
                &["icon"],
                &format!("{path}.presentation"),
                errors,
            );
            required_string(
                presentation,
                "icon",
                &format!("{path}.presentation"),
                errors,
            );
            if let Some(icon) = presentation.get("icon").and_then(Value::as_str)
                && !valid_icon_slug(icon)
            {
                errors.push(
                    format!("{path}.presentation.icon"),
                    "must be a safe lowercase icon slug",
                );
            }
        }
        None => errors.push(format!("{path}.presentation"), "required object"),
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
        errors.push(format!("{path}.kind"), format!("unsupported kind {kind:?}"));
    }
    let slot = item.get("slot").and_then(Value::as_str).unwrap_or("none");
    if kind == "weapon" || kind == "shield" {
        if slot != "any_holding" {
            errors.push(
                format!("{path}.slot"),
                format!("{kind} requires any_holding"),
            );
        }
    } else if kind == "armor"
        && !matches!(slot, "head" | "chest" | "stomach" | "any_arm" | "any_leg")
    {
        errors.push(format!("{path}.slot"), "invalid armor slot");
    }
    if kind == "weapon" {
        validate_weapon(item, &path, errors);
        validate_weapon_carry(item, &path, errors);
    } else {
        if item.contains_key("carry") {
            errors.push(format!("{path}.carry"), "field is only valid for weapon");
        }
        for field in ["handling", "animation_pack"] {
            if item.contains_key(field) {
                errors.push(format!("{path}.{field}"), "field is only valid for weapon");
            }
        }
        for field in [
            "accuracy",
            "preferred_attack",
            "swing_precision",
            "stab_precision",
            "reach_m",
            "penetration",
            "moment_of_inertia_kg_m2",
            "precise",
            "melee",
            "ranged",
            "damage_types",
            "skills",
        ] {
            if item.contains_key(field) {
                errors.push(format!("{path}.{field}"), "field is only valid for weapon");
            }
        }
    }
    if item.contains_key("coverage") {
        errors.push(
            format!("{path}.coverage"),
            "derived from equipment placement surface spans",
        );
    }
    if kind != "armor" {
        for field in ["resistance", "padding", "flexibility", "range_of_motion"] {
            if item.contains_key(field) {
                errors.push(format!("{path}.{field}"), "field is only valid for armor");
            }
        }
    }
    if kind != "shield" && item.contains_key("block") {
        errors.push(format!("{path}.block"), "field is only valid for shield");
    }
    if kind == "shield" {
        finite_in(item, "block", f64::EPSILON, 10_000.0, &path, errors);
    }
    if kind == "armor" {
        for field in ["flexibility", "range_of_motion"] {
            finite_in(item, field, 0.0, 1.0, &path, errors);
        }
        for field in ["resistance", "padding"] {
            finite_in(item, field, 0.0, 1_000_000.0, &path, errors);
        }
    }
    match item.get("equipment") {
        Some(value) => validate_equipment(value, &path, kind, errors),
        None if matches!(kind, "armor" | "clothing") => errors.push(
            format!("{path}.equipment"),
            "armor and clothing require explicit placement and protection mappings",
        ),
        None => {}
    }
    if !matches!(kind, "weapon" | "armor" | "shield" | "container") && item.contains_key("slot") {
        errors.push(format!("{path}.slot"), "kind does not accept a slot");
    }
    if let Some(value) = item.get("capabilities") {
        if let Some(capabilities) = value.as_object() {
            validate_capabilities(capabilities, &path, kind, errors);
        } else {
            errors.push(format!("{path}.capabilities"), "must be an object");
        }
    }
}

fn validate_weapon_carry(
    item: &Map<String, Value>,
    path: &str,
    errors: &mut CatalogDiagnostics<'_>,
) {
    let carry = item.get("carry").and_then(Value::as_str).unwrap_or("");
    if !matches!(carry, "sheathable" | "hand_only") {
        errors.push(
            format!("{path}.carry"),
            "weapon requires sheathable or hand_only",
        );
        return;
    }
    let Some(equipment) = item.get("equipment").and_then(Value::as_object) else {
        errors.push(
            format!("{path}.equipment"),
            "weapon requires explicit hand placements",
        );
        return;
    };
    let placements = equipment
        .get("placements")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let has_parent_placement = placements.iter().any(|placement| {
        placement
            .get("parents")
            .and_then(Value::as_array)
            .is_some_and(|parents| !parents.is_empty())
    });
    let has_sheath_placement = placements.iter().any(|placement| {
        placement
            .get("parents")
            .and_then(Value::as_array)
            .is_some_and(|parents| {
                parents.len() == 1
                    && parents[0].get("channel").and_then(Value::as_str) == Some("containment")
                    && match parents[0].get("order") {
                        None => true,
                        Some(order) => order.as_u64() == Some(0),
                    }
            })
    });
    let hand_only_placements_are_held_roots = placements.iter().all(|placement| {
        let parents_are_empty = match placement.get("parents") {
            None => true,
            Some(parents) => parents.as_array().is_some_and(Vec::is_empty),
        };
        let Some(occupancy) = placement.get("occupancy").and_then(Value::as_array) else {
            return false;
        };
        parents_are_empty
            && occupancy.len() == 1
            && occupancy[0]
                .get("location")
                .and_then(Value::as_str)
                .is_some_and(|location| matches!(location, "left_hand" | "right_hand"))
            && occupancy[0].get("channel").and_then(Value::as_str) == Some("held")
            && match occupancy[0].get("order") {
                None => true,
                Some(order) => order.as_u64() == Some(0),
            }
    });
    let has_sheathable_tag = equipment
        .get("attachment_tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|tag| tag.as_str() == Some("sheathable_weapon"));
    match carry {
        "sheathable" => {
            if !has_sheath_placement || !has_sheathable_tag {
                errors.push(
                    path,
                    "sheathable weapon requires an explicitly compatible single order-zero containment parent placement and sheathable_weapon attachment tag",
                );
            }
        }
        "hand_only" => {
            if has_parent_placement || has_sheathable_tag || !hand_only_placements_are_held_roots {
                errors.push(
                    path,
                    "hand_only weapon placements must each be exactly one left/right hand held root, with no parent placements or sheathable_weapon attachment tag",
                );
            }
        }
        _ => unreachable!(),
    }
}

fn validate_equipment(
    value: &Value,
    item_path: &str,
    item_kind: &str,
    errors: &mut CatalogDiagnostics<'_>,
) {
    let path = format!("{item_path}.equipment");
    let Some(equipment) = value.as_object() else {
        errors.push(&path, "must be an object");
        return;
    };
    reject_unknown(
        equipment,
        &[
            "physical",
            "material",
            "attachment_tags",
            "placements",
            "protection",
            "attachment_points",
        ],
        &path,
        errors,
    );
    let material = equipment.get("material").and_then(Value::as_str);
    let valid_materials = [
        "polished_steel",
        "rough_steel",
        "oxidized_steel",
        "mail_steel",
        "vegetable_tanned_leather",
        "linen",
        "wool",
        "quilted_textile",
    ];
    if matches!(item_kind, "armor" | "clothing") {
        if material.is_none_or(|material| !valid_materials.contains(&material)) {
            errors.push(
                format!("{path}.material"),
                "armor and clothing require a procedural PBR material",
            );
        }
    } else if material.is_some_and(|material| !valid_materials.contains(&material)) {
        errors.push(
            format!("{path}.material"),
            "unknown procedural PBR material",
        );
    }
    match equipment.get("physical").and_then(Value::as_object) {
        Some(physical) => {
            reject_unknown(
                physical,
                &["dimensions_m", "grip_to_tip_m", "anchor_offset_m"],
                &format!("{path}.physical"),
                errors,
            );
            match physical.get("dimensions_m").and_then(Value::as_array) {
                Some(values)
                    if values.len() == 3
                        && values.iter().all(|value| {
                            value
                                .as_f64()
                                .is_some_and(|value| value.is_finite() && value > 0.0)
                        }) => {}
                _ => errors.push(
                    format!("{path}.physical.dimensions_m"),
                    "expected three finite positive metres",
                ),
            }
            let grip_to_tip = physical
                .get("grip_to_tip_m")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            if !grip_to_tip.is_finite() || grip_to_tip < 0.0 {
                errors.push(
                    format!("{path}.physical.grip_to_tip_m"),
                    "expected a finite non-negative distance",
                );
            }
            if physical.get("anchor_offset_m").is_some_and(|offset| {
                offset.as_array().is_none_or(|values| {
                    values.len() != 3
                        || values
                            .iter()
                            .any(|value| value.as_f64().is_none_or(|value| !value.is_finite()))
                })
            }) {
                errors.push(
                    format!("{path}.physical.anchor_offset_m"),
                    "expected three finite metres",
                );
            }
        }
        None => errors.push(format!("{path}.physical"), "required object"),
    }
    let valid_locations: BTreeSet<_> = [
        "head",
        "face",
        "neck",
        "chest",
        "stomach",
        "back",
        "left_shoulder",
        "right_shoulder",
        "left_arm",
        "right_arm",
        "left_hand",
        "right_hand",
        "left_leg",
        "right_leg",
        "left_foot",
        "right_foot",
        "left_belt",
        "right_belt",
        "front_belt",
        "back_belt",
        "left_pocket",
        "right_pocket",
        "back_left_pocket",
        "back_right_pocket",
    ]
    .into_iter()
    .collect();
    let valid_channels: BTreeSet<_> = [
        "held",
        "base_clothing",
        "padding",
        "flexible_armor",
        "rigid_armor",
        "outerwear",
        "accessory",
        "mount",
        "containment",
    ]
    .into_iter()
    .collect();
    match equipment.get("placements").and_then(Value::as_array) {
        Some(placements) if !placements.is_empty() => {
            let mut placement_ids = BTreeSet::new();
            for (index, placement) in placements.iter().enumerate() {
                let Some(placement) = placement.as_object() else {
                    errors.push(format!("{path}.placements.{index}"), "must be an object");
                    continue;
                };
                let placement_path = format!("{path}.placements.{index}");
                reject_unknown(
                    placement,
                    &["id", "occupancy", "parents", "protection", "surface"],
                    &placement_path,
                    errors,
                );
                let id = placement.get("id").and_then(Value::as_str).unwrap_or("");
                if !valid_id(id) || !placement_ids.insert(id) {
                    errors.push(format!("{placement_path}.id"), "must be a unique stable ID");
                }
                let parents = placement
                    .get("parents")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let occupancy = placement
                    .get("occupancy")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if occupancy.is_empty() && parents.is_empty() {
                    errors.push(
                        &placement_path,
                        "must have at least one physical occupancy or parent requirement",
                    );
                }
                let mut occupied = BTreeSet::new();
                for (occupancy_index, requirement) in occupancy.iter().enumerate() {
                    let Some(requirement) = requirement.as_object() else {
                        errors.push(
                            format!("{placement_path}.occupancy.{occupancy_index}"),
                            "must be an object",
                        );
                        continue;
                    };
                    reject_unknown(
                        requirement,
                        &["location", "channel", "order"],
                        &format!("{placement_path}.occupancy.{occupancy_index}"),
                        errors,
                    );
                    let location = requirement
                        .get("location")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let channel = requirement
                        .get("channel")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if !valid_locations.contains(location) {
                        errors.push(
                            format!("{placement_path}.occupancy.{occupancy_index}.location"),
                            "invalid location",
                        );
                    }
                    if !valid_channels.contains(channel) {
                        errors.push(
                            format!("{placement_path}.occupancy.{occupancy_index}.channel"),
                            "invalid channel",
                        );
                    }
                    let order = requirement
                        .get("order")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    if matches!(
                        channel,
                        "held"
                            | "base_clothing"
                            | "padding"
                            | "flexible_armor"
                            | "rigid_armor"
                            | "outerwear"
                    ) && order != 0
                    {
                        errors.push(
                            format!("{placement_path}.occupancy.{occupancy_index}.order"),
                            "singleton channel requires order 0",
                        );
                    }
                    if order > u16::MAX.into() || !occupied.insert((location, channel, order)) {
                        errors.push(
                            format!("{placement_path}.occupancy.{occupancy_index}"),
                            "duplicate or invalid ordered occupancy",
                        );
                    }
                }
                for (parent_index, parent) in parents.iter().enumerate() {
                    let Some(parent) = parent.as_object() else {
                        errors.push(
                            format!("{placement_path}.parents.{parent_index}"),
                            "must be an object",
                        );
                        continue;
                    };
                    reject_unknown(
                        parent,
                        &["channel", "order"],
                        &format!("{placement_path}.parents.{parent_index}"),
                        errors,
                    );
                    if !parent
                        .get("channel")
                        .and_then(Value::as_str)
                        .is_some_and(|channel| valid_channels.contains(channel))
                    {
                        errors.push(
                            format!("{placement_path}.parents.{parent_index}.channel"),
                            "invalid channel",
                        );
                    }
                }
                let mut protected = BTreeSet::new();
                for body_part in placement
                    .get("protection")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    match body_part.as_str() {
                        Some(part)
                            if matches!(
                                part,
                                "left_arm"
                                    | "right_arm"
                                    | "left_leg"
                                    | "right_leg"
                                    | "chest"
                                    | "stomach"
                                    | "head"
                            ) && protected.insert(part) => {}
                        Some(part) => errors.push(
                            format!("{placement_path}.protection"),
                            format!("duplicate or invalid body part {part:?}"),
                        ),
                        None => errors.push(
                            format!("{placement_path}.protection"),
                            "body parts must be strings",
                        ),
                    }
                }
                if matches!(item_kind, "armor" | "clothing") && protected.is_empty() {
                    errors.push(
                        format!("{placement_path}.protection"),
                        "armor and clothing require at least one explicit body part",
                    );
                }
                if !protected.is_empty()
                    && item_kind != "armor"
                    && equipment.get("protection").is_none()
                {
                    errors.push(
                        format!("{placement_path}.protection"),
                        "non-armor protection requires equipment.protection stats",
                    );
                }
                validate_equipment_surface(
                    placement.get("surface"),
                    &placement_path,
                    item_kind,
                    material.is_some(),
                    &protected,
                    errors,
                );
            }
        }
        _ => errors.push(format!("{path}.placements"), "required non-empty array"),
    }
    for field in ["attachment_tags"] {
        if let Some(tags) = equipment.get(field).and_then(Value::as_array) {
            for tag in tags {
                if !tag.as_str().is_some_and(valid_id) {
                    errors.push(format!("{path}.{field}"), "invalid attachment tag");
                }
            }
        }
    }
    if let Some(protection) = equipment.get("protection").and_then(Value::as_object) {
        if item_kind == "armor" {
            errors.push(
                format!("{path}.protection"),
                "armor stats belong only in the armor kind payload",
            );
        }
        reject_unknown(
            protection,
            &["resistance", "padding", "flexibility", "range_of_motion"],
            &format!("{path}.protection"),
            errors,
        );
        if protection.contains_key("coverage") {
            errors.push(
                format!("{path}.protection.coverage"),
                "derived from equipment placement surface spans",
            );
        }
        for field in ["flexibility", "range_of_motion"] {
            if !protection.contains_key(field) {
                continue;
            }
            finite_in(
                protection,
                field,
                0.0,
                1.0,
                &format!("{path}.protection"),
                errors,
            );
        }
        for field in ["resistance", "padding"] {
            if !protection.contains_key(field) {
                continue;
            }
            finite_in(
                protection,
                field,
                0.0,
                1_000_000.0,
                &format!("{path}.protection"),
                errors,
            );
        }
    }
    if let Some(points) = equipment.get("attachment_points").and_then(Value::as_array) {
        let mut ids = BTreeSet::new();
        for (index, point) in points.iter().enumerate() {
            let Some(point) = point.as_object() else {
                errors.push(
                    format!("{path}.attachment_points.{index}"),
                    "must be an object",
                );
                continue;
            };
            reject_unknown(
                point,
                &[
                    "id",
                    "channel",
                    "capacity",
                    "order",
                    "locations",
                    "accepts_tags",
                    "surface_uv",
                    "tangent_direction",
                ],
                &format!("{path}.attachment_points.{index}"),
                errors,
            );
            let id = point.get("id").and_then(Value::as_str).unwrap_or("");
            if !valid_id(id) || !ids.insert(id) {
                errors.push(
                    format!("{path}.attachment_points.{index}.id"),
                    "invalid or duplicate",
                );
            }
            if !point
                .get("channel")
                .and_then(Value::as_str)
                .is_some_and(|channel| valid_channels.contains(channel))
            {
                errors.push(
                    format!("{path}.attachment_points.{index}.channel"),
                    "invalid",
                );
            }
            if point
                .get("capacity")
                .and_then(Value::as_u64)
                .is_none_or(|capacity| capacity == 0 || capacity > u16::MAX.into())
            {
                errors.push(
                    format!("{path}.attachment_points.{index}.capacity"),
                    format!("expected 1..={}", u16::MAX),
                );
            }
            if let Some(locations) = point.get("locations").and_then(Value::as_array) {
                let mut seen = BTreeSet::new();
                for location in locations {
                    if !location.as_str().is_some_and(|location| {
                        valid_locations.contains(location) && seen.insert(location)
                    }) {
                        errors.push(
                            format!("{path}.attachment_points.{index}.locations"),
                            "invalid or duplicate location",
                        );
                    }
                }
            }
            if let Some(tags) = point.get("accepts_tags").and_then(Value::as_array) {
                for tag in tags {
                    if !tag.as_str().is_some_and(valid_id) {
                        errors.push(
                            format!("{path}.attachment_points.{index}.accepts_tags"),
                            "invalid tag",
                        );
                    }
                }
            }
            if point.get("surface_uv").is_some_and(|surface| {
                let Some(surface) = surface.as_object() else {
                    return true;
                };
                if surface
                    .keys()
                    .any(|field| !matches!(field.as_str(), "domain" | "uv"))
                    || surface.len() != 2
                    || !surface
                        .get("domain")
                        .and_then(Value::as_str)
                        .is_some_and(valid_id)
                {
                    return true;
                }
                surface
                    .get("uv")
                    .and_then(Value::as_array)
                    .is_none_or(|values| {
                        values.len() != 2
                            || values.iter().any(|value| {
                                value.as_f64().is_none_or(|value| {
                                    !value.is_finite() || !(0.0..=1.0).contains(&value)
                                })
                            })
                    })
            }) {
                errors.push(
                    format!("{path}.attachment_points.{index}.surface_uv"),
                    "expected {domain, uv} with a valid domain and two finite components in 0..=1",
                );
            }
            if point.get("tangent_direction").is_some_and(|direction| {
                direction.as_array().is_none_or(|values| {
                    if values.len() != 3 {
                        return true;
                    }
                    let components = values.iter().map(Value::as_f64).collect::<Option<Vec<_>>>();
                    components.is_none_or(|components| {
                        components.iter().any(|value| !value.is_finite())
                            || components.iter().map(|value| value * value).sum::<f64>() <= 1e-12
                    })
                })
            }) {
                errors.push(
                    format!("{path}.attachment_points.{index}.tangent_direction"),
                    "expected three finite components with non-zero length",
                );
            }
            if point.get("surface_uv").is_some() != point.get("tangent_direction").is_some() {
                errors.push(
                    format!("{path}.attachment_points.{index}"),
                    "surface_uv and tangent_direction must be authored together",
                );
            }
        }
    }
}

fn validate_equipment_surface(
    value: Option<&Value>,
    placement_path: &str,
    item_kind: &str,
    has_material: bool,
    protected: &BTreeSet<&str>,
    errors: &mut CatalogDiagnostics<'_>,
) {
    let path = format!("{placement_path}.surface");
    let required = matches!(item_kind, "armor" | "clothing");
    let Some(spans) = value.and_then(Value::as_array) else {
        if required {
            errors.push(
                &path,
                "armor and clothing require non-empty anatomical surface spans",
            );
        }
        return;
    };
    if spans.is_empty() {
        if required {
            errors.push(
                &path,
                "armor and clothing require non-empty anatomical surface spans",
            );
        }
        return;
    }
    if !has_material {
        errors.push(
            &path,
            "anatomical surface spans require a procedural PBR material",
        );
    }
    let valid_regions = [
        "head",
        "neck",
        "chest",
        "stomach",
        "left_upper_arm",
        "left_forearm",
        "right_upper_arm",
        "right_forearm",
        "left_thigh",
        "left_lower_leg",
        "right_thigh",
        "right_lower_leg",
    ];
    for (index, span) in spans.iter().enumerate() {
        let span_path = format!("{path}.{index}");
        let Some(span) = span.as_object() else {
            errors.push(&span_path, "must be an object");
            continue;
        };
        reject_unknown(span, &["regions", "anchor", "coverage"], &span_path, errors);
        let regions = span
            .get("regions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if regions.is_empty()
            || regions.iter().any(|region| {
                !region
                    .as_str()
                    .is_some_and(|name| valid_regions.contains(&name))
            })
        {
            errors.push(
                format!("{span_path}.regions"),
                "expected a non-empty anatomical region chain",
            );
            continue;
        }
        let region_names = regions.iter().filter_map(Value::as_str).collect::<Vec<_>>();
        if region_names
            .windows(2)
            .any(|pair| !contiguous_regions(pair[0], pair[1]))
        {
            errors.push(
                format!("{span_path}.regions"),
                "regions must form a proximal-to-distal contiguous chain",
            );
        }
        for region in &region_names {
            let body_part = anatomical_body_part(region);
            if !protected.is_empty() && !protected.contains(body_part) {
                errors.push(
                    format!("{span_path}.regions"),
                    format!("{region:?} is outside placement protection {protected:?}"),
                );
            }
        }
        if !span
            .get("anchor")
            .and_then(Value::as_str)
            .is_some_and(|anchor| matches!(anchor, "proximal" | "distal" | "center"))
        {
            errors.push(
                format!("{span_path}.anchor"),
                "expected proximal, distal, or center",
            );
        }
        finite_in(span, "coverage", f64::EPSILON, 1.0, &span_path, errors);
    }
}

fn anatomical_body_part(region: &str) -> &str {
    match region {
        "head" | "neck" => "head",
        "chest" => "chest",
        "stomach" => "stomach",
        "left_upper_arm" | "left_forearm" => "left_arm",
        "right_upper_arm" | "right_forearm" => "right_arm",
        "left_thigh" | "left_lower_leg" => "left_leg",
        "right_thigh" | "right_lower_leg" => "right_leg",
        _ => "",
    }
}

fn contiguous_regions(proximal: &str, distal: &str) -> bool {
    matches!(
        (proximal, distal),
        ("stomach", "chest")
            | ("chest", "neck")
            | ("neck", "head")
            | ("left_upper_arm", "left_forearm")
            | ("right_upper_arm", "right_forearm")
            | ("left_thigh", "left_lower_leg")
            | ("right_thigh", "right_lower_leg")
    )
}

fn validate_weapon(item: &Map<String, Value>, path: &str, errors: &mut CatalogDiagnostics<'_>) {
    if !matches!(
        item.get("handling").and_then(Value::as_str),
        Some("one_handed" | "two_handed")
    ) {
        errors.push(
            format!("{path}.handling"),
            "expected one_handed or two_handed",
        );
    }
    if let Some(pack) = item.get("animation_pack")
        && pack.as_str().is_none_or(|pack| !valid_id(pack))
    {
        errors.push(
            format!("{path}.animation_pack"),
            "must be a safe lowercase animation pack ID",
        );
    }
    for field in [
        "accuracy",
        "reach_m",
        "penetration",
        "moment_of_inertia_kg_m2",
    ] {
        finite_in(item, field, 0.0, 10_000.0, path, errors);
    }
    let melee = item.get("melee").and_then(Value::as_bool).unwrap_or(false);
    let ranged = item.get("ranged").and_then(Value::as_bool).unwrap_or(false);
    if melee {
        for field in ["swing_precision", "stab_precision"] {
            finite_in(item, field, 0.0, 10_000.0, path, errors);
            if item
                .get(field)
                .and_then(Value::as_f64)
                .is_none_or(|value| value <= 0.0)
            {
                errors.push(
                    format!("{path}.{field}"),
                    "melee weapons require an explicit positive value",
                );
            }
        }
        if !matches!(
            item.get("preferred_attack").and_then(Value::as_str),
            Some("swing" | "stab")
        ) {
            errors.push(format!("{path}.preferred_attack"), "expected swing or stab");
        }
    }
    if !melee && !ranged {
        errors.push(path, "weapon must be melee and/or ranged");
    }
    let damage = item.get("damage_types").and_then(Value::as_array);
    if damage.is_none_or(Vec::is_empty)
        || damage.is_some_and(|values| {
            values
                .iter()
                .any(|value| !matches!(value.as_str(), Some("blunt" | "slash" | "pierce")))
        })
    {
        errors.push(
            format!("{path}.damage_types"),
            "explicit non-empty supported damage types required",
        );
    }
    let Some(skills) = item.get("skills").and_then(Value::as_object) else {
        errors.push(
            format!("{path}.skills"),
            "explicit normalized distribution required",
        );
        return;
    };
    reject_unknown(
        skills,
        &[
            "polearm", "axe", "bludgeon", "sword", "knife", "bow", "crossbow", "firearm", "throw",
        ],
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
        errors.push(
            format!("{path}.skills"),
            "weights must be finite, non-negative, and sum to 1",
        );
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
        errors.push(
            format!("{path}.skills"),
            "distribution is incompatible with weapon mode",
        );
    }
}

fn validate_capabilities(
    capabilities: &Map<String, Value>,
    path: &str,
    kind: &str,
    errors: &mut CatalogDiagnostics<'_>,
) {
    reject_unknown(
        capabilities,
        &["durability", "food", "alcohol", "container", "book"],
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
            &format!("{path}.capabilities.durability"),
            errors,
        );
        let quality = durability
            .get("quality")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if !(1..=5).contains(&quality) {
            errors.push(
                format!("{path}.capabilities.durability.quality"),
                "expected 1..5",
            );
        }
        for field in ["yield_j", "fracture_j"] {
            finite_in(durability, field, 0.0, 1_000_000.0, path, errors);
        }
        for field in [
            "wear",
            "failure_share",
            "edge_sensitivity",
            "handling_sensitivity",
        ] {
            finite_in(durability, field, 0.0, 1.0, path, errors);
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
            errors.push(
                format!("{path}.capabilities.durability"),
                "yield must be positive and fracture must be at least yield",
            );
        }
    } else if matches!(kind, "weapon" | "armor" | "shield" | "clothing") {
        errors.push(
            format!("{path}.capabilities.durability"),
            "required for repairable kind",
        );
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
            errors.push(
                format!("{path}.capabilities.food.class"),
                "unsupported food class",
            );
        }
        for field in ["nutrition_kcal", "value_per_unit", "growth_per_hour"] {
            finite_in(food, field, 0.0, 1_000_000.0, path, errors);
        }
        let quality = food.get("quality").and_then(Value::as_u64).unwrap_or(0);
        if !(1..=5).contains(&quality) {
            errors.push(format!("{path}.capabilities.food.quality"), "expected 1..5");
        }
        match food.get("flavors_kg").and_then(Value::as_object) {
            Some(flavors) => {
                reject_unknown(
                    flavors,
                    &["salty", "spicy", "sweet", "sour", "savory"],
                    &format!("{path}.capabilities.food.flavors_kg"),
                    errors,
                );
                for field in ["salty", "spicy", "sweet", "sour", "savory"] {
                    finite_in(flavors, field, 0.0, 1_000.0, path, errors);
                }
            }
            None => errors.push(
                format!("{path}.capabilities.food.flavors_kg"),
                "required object",
            ),
        }
    } else if kind == "food" {
        errors.push(
            format!("{path}.capabilities.food"),
            "food kind requires nutrition metadata",
        );
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
        if serving == 0
            || serving > 10_000
            || abv > u64::from(BASIS_POINTS_PER_WHOLE)
            || hydration > serving
        {
            errors.push(
                format!("{path}.capabilities.alcohol"),
                "invalid ml/ABV values",
            );
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
            errors.push(
                format!("{path}.capabilities.alcohol"),
                "missing or invalid behavior metadata",
            );
        }
    }
    if let Some(container) = capabilities.get("container").and_then(Value::as_object) {
        reject_unknown(
            container,
            &["capacity_ml"],
            &format!("{path}.capabilities.container"),
            errors,
        );
        let capacity = container
            .get("capacity_ml")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if capacity == 0 || capacity > 1_000_000 {
            errors.push(
                format!("{path}.capabilities.container.capacity_ml"),
                "expected 1..1000000",
            );
        }
    }
    if kind == "container" && !capabilities.contains_key("container") {
        errors.push(
            format!("{path}.capabilities.container"),
            "container kind requires capacity metadata",
        );
    }
    if let Some(book) = capabilities.get("book").and_then(Value::as_object) {
        reject_unknown(
            book,
            &["medium", "target", "quality", "settlement_allowlist"],
            &format!("{path}.capabilities.book"),
            errors,
        );
        if kind != "simple" {
            errors.push(
                format!("{path}.capabilities.book"),
                "books must use kind simple",
            );
        }
        let parsed =
            serde_json::from_value::<crate::item_catalog_schema::Book>(Value::Object(book.clone()));
        match parsed {
            Ok(book) if valid_book_shape(&book) => {}
            Ok(_) => errors.push(
                format!("{path}.capabilities.book"),
                "target must be a supported leaf with a legal quality",
            ),
            Err(error) => errors.push(format!("{path}.capabilities.book"), error.to_string()),
        }
    }
}

fn valid_book_shape(book: &crate::item_catalog_schema::Book) -> bool {
    use crate::item_catalog_schema::BookTarget;
    let mut settlements = BTreeSet::new();
    if book.settlement_allowlist.len() > 64
        || book.settlement_allowlist.iter().any(|id| {
            id.is_empty()
                || id.len() > 96
                || !id.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                || !settlements.insert(id)
        })
    {
        return false;
    }
    let maximum = match &book.target {
        BookTarget::Written { .. } | BookTarget::Religion { .. } | BookTarget::Bestiary { .. } => 5,
        BookTarget::Skill { skill } if matches!(skill.as_str(), "physiology" | "herbalism") => 4,
        BookTarget::Terrain { terrain }
            if matches!(
                terrain.as_str(),
                "plains" | "forest" | "hills" | "wetlands" | "urban" | "snow"
            ) =>
        {
            2
        }
        BookTarget::Skill { skill }
            if matches!(
                skill.as_str(),
                "surgery" | "cooking" | "tailoring" | "smithing" | "command" | "charm"
            ) =>
        {
            2
        }
        BookTarget::Skill { skill }
            if matches!(
                skill.as_str(),
                "polearm"
                    | "axe"
                    | "bludgeon"
                    | "sword"
                    | "knife"
                    | "bow"
                    | "crossbow"
                    | "firearm"
                    | "throw"
                    | "dodge"
                    | "block"
                    | "balance"
                    | "stealth"
            ) =>
        {
            1
        }
        _ => return false,
    };
    (1..=maximum).contains(&book.quality)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_icon_slug(icon: &str) -> bool {
    !icon.is_empty()
        && icon.len() <= 64
        && !icon.starts_with('-')
        && !icon.ends_with('-')
        && icon
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn required_string(
    object: &Map<String, Value>,
    field: &str,
    path: &str,
    errors: &mut CatalogDiagnostics<'_>,
) {
    if object
        .get(field)
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        errors.push(format!("{path}.{field}"), "required non-empty string");
    }
}

fn finite_in(
    object: &Map<String, Value>,
    field: &str,
    min: f64,
    max: f64,
    path: &str,
    errors: &mut CatalogDiagnostics<'_>,
) {
    if object
        .get(field)
        .and_then(Value::as_f64)
        .is_none_or(|value| !value.is_finite() || value < min || value > max)
    {
        errors.push(
            format!("{path}.{field}"),
            format!("expected finite value in {min}..={max}"),
        );
    }
}

fn reject_unknown(
    object: &Map<String, Value>,
    allowed: &[&str],
    path: &str,
    errors: &mut CatalogDiagnostics<'_>,
) {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            errors.push(format!("{path}.{key}"), "unknown field");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn collect_diagnostics(
        validate: impl FnOnce(&mut CatalogDiagnostics<'_>),
    ) -> Vec<CatalogDiagnostic> {
        let mut diagnostics = Vec::new();
        validate(&mut CatalogDiagnostics {
            source_index: 0,
            diagnostics: &mut diagnostics,
        });
        diagnostics
    }

    fn valid_item(id: &str) -> Value {
        json!({
            "id": id,
            "display_name": "Test",
            "weight_kg": 1.0,
            "exterior_volume_ml": 1250,
            "base_value": 1,
            "tags": [],
            "presentation": {"icon": "help"},
            "kind": "simple"
        })
    }

    fn document(items: Vec<Value>) -> Value {
        json!({"schema_version": 1, "items": items})
    }

    fn diagnostic_contains(diagnostic: &CatalogDiagnostic, value: &str) -> bool {
        diagnostic.path.contains(value) || diagnostic.message.contains(value)
    }

    fn validation_contains(error: &CatalogValidationError, value: &str) -> bool {
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic_contains(diagnostic, value))
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
        assert!(validation_contains(&error, "schema_version"));
        assert!(validation_contains(&error, "unknown field"));
        assert!(validation_contains(&error, "duplicate stable ID"));
        let rendered = error.to_string();
        assert!(rendered.contains("content/items/a.yaml"));
        assert!(rendered.contains("content/items/b.yaml"));
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
            ("moment_of_inertia_kg_m2".into(), json!(1)),
            ("precise".into(), json!(false)),
            ("melee".into(), json!(true)),
            ("ranged".into(), json!(false)),
            ("damage_types".into(), json!([])),
            ("skills".into(), json!({"laser": 1.0})),
        ]);
        let error =
            validate_documents(&[document(vec![weapon])], &["fixture.yaml".into()]).unwrap_err();
        for (path, message) in [
            (
                "item BAD-ID.id",
                "must be 1..64 lowercase ASCII letters, digits, or underscores",
            ),
            (
                "item BAD-ID.weight_kg",
                "expected finite value in 0..=10000",
            ),
            ("item BAD-ID.slot", "weapon requires any_holding"),
            (
                "item BAD-ID.damage_types",
                "explicit non-empty supported damage types required",
            ),
            ("item BAD-ID.skills.laser", "unknown field"),
            (
                "item BAD-ID.skills",
                "weights must be finite, non-negative, and sum to 1",
            ),
            (
                "item BAD-ID.skills",
                "distribution is incompatible with weapon mode",
            ),
        ] {
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.path == path && diagnostic.message == message),
                "missing structured diagnostic ({path:?}, {message:?}): {error}"
            );
        }
    }

    #[test]
    fn semantic_diagnostics_include_exact_source_coordinates() {
        let source = "{\n  \"schema_version\": 1,\n  \"items\": [{\n    \"id\": \"bad\",\n    \"display_name\": \"Bad\",\n    \"weight_kg\": -1,\n    \"base_value\": 1,\n    \"tags\": [],\n    \"presentation\": {\"icon\": \"help\"},\n    \"kind\": \"simple\"\n  }]\n}";
        let value = parse_document(source).unwrap();
        let error = validate_documents_with_sources(
            &[value],
            &["content/items/test.yaml".into()],
            &[source.into()],
        )
        .unwrap_err();
        assert!(error.diagnostics.iter().any(|diagnostic| {
            diagnostic.source_index == 0
                && diagnostic.path == "item bad.weight_kg"
                && diagnostic.message.starts_with("expected finite value")
        }));
        assert!(
            error
                .to_string()
                .contains("content/items/test.yaml:6:5: item bad.weight_kg")
        );
    }

    #[test]
    fn typed_schema_rejects_integer_overflow_and_missing_required_fields() {
        let mut item = valid_item("typed");
        item.as_object_mut()
            .unwrap()
            .insert("base_value".into(), json!(u64::MAX));
        let error =
            validate_documents(&[document(vec![item])], &["typed.yaml".into()]).unwrap_err();
        assert!(validation_contains(&error, "typed schema mismatch"));
    }

    #[test]
    fn mechanics_backed_kinds_are_closed_sets() {
        let mut item = valid_item("unsupported_coin");
        item.as_object_mut()
            .unwrap()
            .insert("kind".into(), json!("currency"));
        let error =
            validate_documents(&[document(vec![item])], &["currency.yaml".into()]).unwrap_err();
        assert!(validation_contains(
            &error,
            "currency IDs must match the supported gameplay registry"
        ));
    }

    #[test]
    fn book_capabilities_require_supported_quality() {
        let valid = json!({
            "book": {
                "medium": "German",
                "target": {"kind": "written", "language": "Latin"},
                "quality": 1
            }
        });
        let errors = collect_diagnostics(|errors| {
            validate_capabilities(
                valid.as_object().unwrap(),
                "item latin_primer",
                "simple",
                errors,
            );
        });
        assert!(errors.is_empty(), "{errors:?}");

        let excessive = json!({
            "book": {
                "medium": "German",
                "target": {"kind": "skill", "skill": "sword"},
                "quality": 2
            }
        });
        let errors = collect_diagnostics(|errors| {
            validate_capabilities(
                excessive.as_object().unwrap(),
                "item master_sword_book",
                "simple",
                errors,
            );
        });
        assert!(
            errors
                .iter()
                .any(|error| diagnostic_contains(error, "legal quality"))
        );
    }

    #[test]
    fn equipment_placements_allow_mixed_anchors_and_reject_unknown_locations() {
        let mut item = valid_item("bad_sleeve");
        item.as_object_mut().unwrap().extend([
            ("kind".into(), json!("clothing")),
            (
                "equipment".into(),
                json!({
                    "placements": [
                        {
                            "id": "bad",
                            "occupancy": [{"location": "dragon_wing", "channel": "padding"}],
                            "parents": [{"channel": "mount"}]
                        }
                    ]
                }),
            ),
        ]);
        let error =
            validate_documents(&[document(vec![item])], &["equipment.yaml".into()]).unwrap_err();
        assert!(!validation_contains(
            &error,
            "at least one physical occupancy or parent requirement"
        ));
        assert!(validation_contains(&error, "invalid location"));
    }

    #[test]
    fn explicit_sided_placement_deserializes_as_two_atomic_alternatives() {
        let mut item = valid_item("good_sleeve");
        item.as_object_mut().unwrap().extend([
            ("kind".into(), json!("clothing")),
            (
                "equipment".into(),
                json!({
                    "material": "quilted_textile",
                    "physical": {
                        "dimensions_m": [0.3, 0.6, 0.1],
                        "grip_to_tip_m": 0.0,
                        "anchor_offset_m": [0.0, 0.0, 0.0]
                    },
                    "placements": [
                        {
                            "id": "left",
                            "occupancy": [{"location": "left_arm", "channel": "base_clothing"}],
                            "protection": ["left_arm"],
                            "surface": [{
                                "regions": ["left_upper_arm", "left_forearm"],
                                "anchor": "proximal",
                                "coverage": 0.8
                            }]
                        },
                        {
                            "id": "right",
                            "occupancy": [{"location": "right_arm", "channel": "base_clothing"}],
                            "protection": ["right_arm"],
                            "surface": [{
                                "regions": ["right_upper_arm", "right_forearm"],
                                "anchor": "proximal",
                                "coverage": 0.8
                            }]
                        }
                    ],
                    "protection": {"padding": 2.0, "resistance": 1.0}
                }),
            ),
        ]);
        let documents = [document(vec![item])];
        let equipment_value = documents[0]["items"][0]["equipment"].clone();
        let errors = collect_diagnostics(|errors| {
            validate_equipment(&equipment_value, "items.0", "clothing", errors);
        });
        assert!(errors.is_empty(), "{errors:#?}");
        let compiled: crate::item_catalog_schema::ItemCatalogDocument =
            serde_json::from_value(documents[0].clone()).expect("typed catalog");
        let equipment = compiled.items[0].equipment.as_ref().expect("equipment");
        assert_eq!(equipment.placements.len(), 2);
        assert_eq!(
            equipment.placements[0].occupancy[0].location,
            crate::item_catalog_schema::EquipmentLocation::LeftArm
        );
    }

    #[test]
    fn fitted_accessory_may_author_a_surface_without_protection() {
        let mut item = valid_item("fitted_belt");
        item.as_object_mut().unwrap().insert(
            "equipment".into(),
            json!({
                "material": "vegetable_tanned_leather",
                "physical": {
                    "dimensions_m": [0.42, 0.10, 0.24],
                    "grip_to_tip_m": 0.0,
                    "anchor_offset_m": [0.0, 0.0, 0.0]
                },
                "placements": [{
                    "id": "worn",
                    "occupancy": [{"location": "left_belt", "channel": "accessory"}],
                    "surface": [{
                        "regions": ["stomach"],
                        "anchor": "center",
                        "coverage": 0.18
                    }]
                }],
                "attachment_points": [{
                    "id": "left",
                    "channel": "mount",
                    "capacity": 1,
                    "locations": ["left_belt"],
                    "surface_uv": {"domain": "mhr_body_v1", "uv": [0.37, 0.71]},
                    "tangent_direction": [0.0, -0.82, -0.57]
                }]
            }),
        );

        let equipment_value = item["equipment"].clone();
        let errors = collect_diagnostics(|errors| {
            validate_equipment(&equipment_value, "items.0", "simple", errors);
        });
        assert!(errors.is_empty(), "{errors:#?}");
        let documents = [document(vec![item])];
        let compiled: crate::item_catalog_schema::ItemCatalogDocument =
            serde_json::from_value(documents[0].clone()).expect("typed catalog");
        let belt = &compiled.items[0];
        assert!(belt.equipment.as_ref().is_some_and(|equipment| {
            equipment.material.is_some() && !equipment.placements[0].surface.is_empty()
        }));
        let point = &belt.equipment.as_ref().unwrap().attachment_points[0];
        assert_eq!(
            point
                .surface_uv
                .as_ref()
                .map(|surface| surface.domain.as_str()),
            Some("mhr_body_v1")
        );

        let mut unpaired = equipment_value;
        unpaired["attachment_points"][0]
            .as_object_mut()
            .unwrap()
            .remove("tangent_direction");
        let errors = collect_diagnostics(|errors| {
            validate_equipment(&unpaired, "items.0", "simple", errors);
        });
        assert!(errors.iter().any(|error| {
            diagnostic_contains(
                error,
                "surface_uv and tangent_direction must be authored together",
            )
        }));
    }

    #[test]
    fn protection_defaults_are_safe_and_singleton_channels_reject_nonzero_order() {
        let mut item = valid_item("defaulted_clothing");
        item.as_object_mut().unwrap().extend([
            ("kind".into(), json!("clothing")),
            (
                "equipment".into(),
                json!({
                    "physical": {
                        "dimensions_m": [0.3, 0.6, 0.1],
                        "grip_to_tip_m": 0.0,
                        "anchor_offset_m": [0.0, 0.0, 0.0]
                    },
                    "placements": [{
                        "id": "worn",
                        "occupancy": [{
                            "location": "chest",
                            "channel": "base_clothing"
                        }],
                        "protection": ["chest"]
                    }],
                    "protection": {"coverage": 0.5}
                }),
            ),
        ]);
        let documents = [document(vec![item.clone()])];
        let compiled: crate::item_catalog_schema::ItemCatalogDocument =
            serde_json::from_value(documents[0].clone()).expect("typed catalog");
        let protection = compiled.items[0]
            .equipment
            .as_ref()
            .and_then(|equipment| equipment.protection)
            .expect("protection");
        assert_eq!(protection.padding, 0.0);
        assert_eq!(protection.resistance, 0.0);
        assert_eq!(protection.flexibility, 1.0);
        assert_eq!(protection.range_of_motion, 1.0);

        item["equipment"]["placements"][0]["occupancy"][0]["order"] = json!(1);
        let error =
            validate_documents(&[document(vec![item])], &["equipment.yaml".into()]).unwrap_err();
        assert!(validation_contains(
            &error,
            "singleton channel requires order 0"
        ));
    }

    #[test]
    fn weapon_carry_contract_rejects_hand_only_parent_placements() {
        let mut weapon = json!({
            "carry": "hand_only",
            "equipment": {
                "attachment_tags": ["weapon"],
                "placements": [{"id": "contained", "parents": [{"channel": "containment"}]}]
            }
        });
        let errors = collect_diagnostics(|errors| {
            validate_weapon_carry(weapon.as_object().unwrap(), "items.0", errors);
        });
        assert!(
            errors
                .iter()
                .any(|error| diagnostic_contains(error, "hand_only weapon placements must"))
        );

        weapon["carry"] = json!("sheathable");
        weapon["equipment"]["attachment_tags"] = json!(["weapon", "sheathable_weapon"]);
        let errors = collect_diagnostics(|errors| {
            validate_weapon_carry(weapon.as_object().unwrap(), "items.0", errors);
        });
        assert!(errors.is_empty(), "{errors:#?}");
    }

    #[test]
    fn weapon_carry_contract_rejects_non_hand_held_hand_only_roots() {
        for occupancy in [
            json!([{"location": "chest", "channel": "held"}]),
            json!([{"location": "left_hand", "channel": "accessory"}]),
            json!([
                {"location": "left_hand", "channel": "held"},
                {"location": "right_hand", "channel": "held"}
            ]),
        ] {
            let weapon = json!({
                "carry": "hand_only",
                "equipment": {
                    "attachment_tags": ["weapon"],
                    "placements": [{"id": "invalid", "occupancy": occupancy}]
                }
            });
            let errors = collect_diagnostics(|errors| {
                validate_weapon_carry(weapon.as_object().unwrap(), "items.0", errors);
            });
            assert!(
                errors.iter().any(|error| diagnostic_contains(
                    error,
                    "exactly one left/right hand held root"
                )),
                "{errors:#?}"
            );
        }
    }

    #[test]
    fn sheathable_contract_requires_single_containment_parent_placement() {
        for parents in [
            json!([{"channel": "mount"}]),
            json!([{"channel": "containment", "order": 1}]),
            json!([
                {"channel": "containment"},
                {"channel": "containment"}
            ]),
        ] {
            let weapon = json!({
                "carry": "sheathable",
                "equipment": {
                    "attachment_tags": ["weapon", "sheathable_weapon"],
                    "placements": [{"id": "invalid", "parents": parents}]
                }
            });
            let errors = collect_diagnostics(|errors| {
                validate_weapon_carry(weapon.as_object().unwrap(), "items.0", errors);
            });
            assert!(
                errors.iter().any(|error| diagnostic_contains(
                    error,
                    "single order-zero containment parent placement"
                )),
                "{errors:#?}"
            );
        }
    }
}
