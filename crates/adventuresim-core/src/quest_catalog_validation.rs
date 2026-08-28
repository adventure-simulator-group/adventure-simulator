//! Dependency-light validation shared verbatim by `build.rs`, runtime startup,
//! and the authoring checker.

use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MAX_BESTIARY_INTERPRETATION_BYTES: usize = 1_024;
pub(crate) const MAX_QUEST_DIALOGUE_TEMPLATE_CHARS: usize = 1_024;

const ROOT_KEYS: &[&str] = &[
    "monsters",
    "evidence",
    "witness_demographics",
    "circumstances",
    "sites",
    "descriptions",
    "templates",
    "consequences",
    "relations",
    "bridges",
    "dialogue_variants",
];
const DIALOGUE_VARIANT_KEYS: &[&str] = &["id", "kind", "priority", "conditions", "template"];
const MONSTER_KEYS: &[&str] = &[
    "id",
    "name",
    "singular",
    "plural",
    "aliases",
    "base_weight",
    "curation_weight",
    "northern_germany_prior",
    "primary_category",
    "secondary_categories",
    "combat",
    "negotiation",
    "investigation",
];
const NEGOTIATION_KEYS: &[&str] = &["sapient", "negotiable"];
const COMBAT_KEYS: &[&str] = &[
    "rig",
    "speed_m_per_minute",
    "weight_kg",
    "attack",
    "ranged",
    "precision_bonus_milli",
    "training_multiplier_milli",
    "perception",
    "stealth",
    "morale",
    "protection",
    "resistance_joules",
    "padding_joules",
    "disease_risk",
    "fear",
    "temperament",
    "encounter_scale_basis_points",
    "loot_item_id",
    "escalation_mode",
    "escalation_growth_rate_bps",
    "baseline_combat_power",
];
const INVESTIGATION_KEYS: &[&str] = &[
    "habitats",
    "activity",
    "victim_tags",
    "tracks",
    "wounds",
    "disturbances",
    "sounds",
    "silhouettes",
    "odors",
    "mistaken_for",
    "distinguishing_clues",
    "preparation_advice",
    "investigability",
    "identification_challenge",
    "location_challenge",
    "countermeasure_hypotheses",
];
const EVIDENCE_KEYS: &[&str] = &[
    "id",
    "portrait_label",
    "portrait_icon",
    "base_description",
    "topics",
];
const TOPIC_KEYS: &[&str] = &["id", "label", "inspection_description", "check", "bestiary"];
const CHECK_KEYS: &[&str] = &[
    "stat",
    "difficulty_min_milli",
    "difficulty_max_milli",
    "success_description",
    "reveals_clue",
];
const BESTIARY_IMPLICATION_KEYS: &[&str] = &[
    "category",
    "support_bps",
    "lore_difficulty_milli",
    "diagnostic_kind",
    "interpretation",
];
const BESTIARY_CATEGORY_IDS: &[&str] = &[
    "beast",
    "undead",
    "human",
    "werekin",
    "elf",
    "dwarf",
    "fey",
    "spirit",
    "greenskin",
    "insectoid",
    "draconid",
    "construct",
    "wildmen",
];
const DEMOGRAPHIC_KEYS: &[&str] = &["id", "label", "match_rules"];
const MATCH_RULE_KEYS: &[&str] = &[
    "priority",
    "age_bands",
    "sexes",
    "professions",
    "local_roles",
    "fallback",
];
const CIRCUMSTANCE_KEYS: &[&str] = &["id", "statement"];
const SITE_KEYS: &[&str] = &["id", "label", "terrain", "habitat"];
const DESCRIPTION_KEYS: &[&str] = &["id", "text"];
const TEMPLATE_KEYS: &[&str] = &[
    "id",
    "label",
    "routes",
    "objectives",
    "cause_finales",
    "consequence_profile",
    "incident_interval_minutes",
    "maximum_incidents",
];
const CONSEQUENCE_KEYS: &[&str] = &[
    "id",
    "family",
    "causes",
    "symptom",
    "buy_bps",
    "sell_penalty_bps",
    "encounter_frequency_bps",
    "encounter_archetype",
    "disease_intensity",
    "public_summary",
];
const RELATION_KEYS: &[&str] = &["id", "candidates"];
const CANDIDATE_KEYS: &[&str] = &[
    "id",
    "plausibility",
    "curation",
    "hard_zero_reason",
    "required_bridge",
    "factors",
];
const BRIDGE_KEYS: &[&str] = &[
    "id",
    "explanation",
    "lead_summary",
    "event_suffix",
    "evidence_id",
    "action_ids",
];

fn object<'a>(value: &'a Value, at: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{at}: expected object"))
}
fn array<'a>(value: &'a Value, at: &str) -> Result<&'a [Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{at}: expected array"))
}
fn string<'a>(object: &'a Map<String, Value>, key: &str, at: &str) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{at}.{key}: expected string"))
}
fn keys(object: &Map<String, Value>, allowed: &[&str], at: &str) -> Result<(), String> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("{at}.{key}: unknown field"));
        }
    }
    Ok(())
}
fn id(value: &str, at: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 63
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'_' | b'-' | b'.' | b':')
        })
    {
        return Err(format!("{at}: invalid bounded catalog ID {value:?}"));
    }
    Ok(())
}
fn enum_value(value: &str, allowed: &[&str], at: &str) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("{at}: unknown mechanic {value}"))
    }
}
fn unsigned(
    object: &Map<String, Value>,
    key: &str,
    min: u64,
    max: u64,
    at: &str,
) -> Result<u64, String> {
    let value = object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{at}.{key}: expected unsigned integer"))?;
    if !(min..=max).contains(&value) {
        return Err(format!("{at}.{key}: value {value} outside {min}..={max}"));
    }
    Ok(value)
}
fn signed(
    object: &Map<String, Value>,
    key: &str,
    min: i64,
    max: i64,
    at: &str,
) -> Result<i64, String> {
    let value = object
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("{at}.{key}: expected integer"))?;
    if !(min..=max).contains(&value) {
        return Err(format!("{at}.{key}: value {value} outside {min}..={max}"));
    }
    Ok(value)
}
fn boolean(object: &Map<String, Value>, key: &str, at: &str) -> Result<bool, String> {
    object
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{at}.{key}: expected boolean"))
}
fn nonempty_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    at: &str,
) -> Result<&'a str, String> {
    let value = string(object, key, at)?;
    if value.trim().is_empty() {
        Err(format!("{at}.{key}: must not be empty"))
    } else {
        Ok(value)
    }
}
fn optional_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    at: &str,
) -> Result<Option<&'a str>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(format!("{at}.{key}: expected string or null")),
    }
}
fn list_strings(object: &Map<String, Value>, key: &str, at: &str) -> Result<Vec<String>, String> {
    array(
        object
            .get(key)
            .ok_or_else(|| format!("{at}.{key}: missing"))?,
        &format!("{at}.{key}"),
    )?
    .iter()
    .enumerate()
    .map(|(index, value)| {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("{at}.{key}[{index}]: expected string"))
    })
    .collect()
}
fn optional_list_strings(
    object: &Map<String, Value>,
    key: &str,
    at: &str,
) -> Result<Vec<String>, String> {
    match object.get(key) {
        None => Ok(Vec::new()),
        Some(value) => array(value, &format!("{at}.{key}"))?
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{at}.{key}[{index}]: expected string"))
            })
            .collect(),
    }
}

fn validate_dialogue_condition(value: &Value, at: &str) -> Result<(), String> {
    let condition = object(value, at)?;
    match string(condition, "op", at)? {
        "always" => Ok(()),
        "all" | "any" => array(
            condition
                .get("conditions")
                .ok_or_else(|| format!("{at}.conditions: missing"))?,
            at,
        )?
        .iter()
        .enumerate()
        .try_for_each(|(index, child)| {
            validate_dialogue_condition(child, &format!("{at}.conditions[{index}]"))
        }),
        "not" => validate_dialogue_condition(
            condition
                .get("condition")
                .ok_or_else(|| format!("{at}.condition: missing"))?,
            &format!("{at}.condition"),
        ),
        "fact" => {
            let key = object(
                condition
                    .get("key")
                    .ok_or_else(|| format!("{at}.key: missing"))?,
                &format!("{at}.key"),
            )?;
            // The runtime deserializes this into the shared dialogue
            // `Condition`, which is the authoritative vocabulary check. Keep
            // this structural validator vocabulary-neutral so quest variants
            // retain parity as new safe dialogue facts are introduced.
            let _kind = string(key, "kind", &format!("{at}.key"))?;
            if !condition.contains_key("equals") {
                return Err(format!("{at}.equals: missing"));
            }
            Ok(())
        }
        other => Err(format!("{at}.op: unknown dialogue condition {other}")),
    }
}

#[derive(Clone)]
struct DemographicRule {
    demographic: String,
    at: String,
    priority: i32,
    age_bands: Vec<String>,
    sexes: Vec<String>,
    professions: Vec<String>,
    local_roles: Vec<String>,
    fallback: bool,
}

pub(crate) fn selector_matches_fact(selector: &str, fact: &str) -> bool {
    let selector = selector.trim().to_ascii_lowercase();
    let fact = fact.trim().to_ascii_lowercase();
    selector == fact
        || fact
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|token| !token.is_empty() && token == selector)
}

const PROFESSION_FACTS: &[&str] = &[
    "artisan",
    "householder",
    "laborer",
    "retainer",
    "service provider",
    "merchant",
    "weaponsmith",
    "armourer",
    "tailor",
    "herbalist",
    "innkeeper",
    "cleric",
    "guard",
    "soldier",
    "noble",
];
const LOCAL_ROLE_FACTS: &[&str] = &[
    "market steward",
    "master weaponsmith",
    "master armourer",
    "master tailor",
    "local healer",
    "innkeeper",
    "parish priest",
    "customer or visitor",
    "neighbor",
    "household representative",
    "local resident",
    "resident",
    "lord's household retainer",
    "keep servant",
];

fn selectors_overlap(left: &[String], right: &[String], facts: &[&str]) -> bool {
    left.is_empty()
        || right.is_empty()
        || facts.iter().any(|fact| {
            left.iter()
                .any(|selector| selector_matches_fact(selector, fact))
                && right
                    .iter()
                    .any(|selector| selector_matches_fact(selector, fact))
        })
}

pub fn validate_documents(documents: &[Value], files: &[String]) -> Result<(), String> {
    if documents.len() != files.len() || documents.is_empty() {
        return Err("catalog: document/source mismatch or empty catalog".into());
    }
    let mut namespaces = BTreeMap::<&str, BTreeSet<String>>::new();
    let mut relations =
        BTreeMap::<String, Vec<(String, u64, u64, Option<String>, Option<String>)>>::new();
    let mut monster_refs = Vec::new();
    let mut description_support = Vec::new();
    let mut bridge_refs = Vec::new();
    let mut demographic_rules = Vec::new();
    let mut template_profiles = Vec::new();
    let mut consequence_families = BTreeMap::<String, BTreeSet<String>>::new();
    let mut fallback_demographics = 0usize;
    for (document_index, document) in documents.iter().enumerate() {
        let file = &files[document_index];
        let root = object(document, file)?;
        keys(root, ROOT_KEYS, file)?;
        for root_key in ROOT_KEYS {
            let Some(items) = root.get(*root_key) else {
                continue;
            };
            for (index, item) in array(items, &format!("{file}.{root_key}"))?
                .iter()
                .enumerate()
            {
                let at = format!("{file}.{root_key}[{index}]");
                let item = object(item, &at)?;
                let allowed = match *root_key {
                    "monsters" => MONSTER_KEYS,
                    "evidence" => EVIDENCE_KEYS,
                    "witness_demographics" => DEMOGRAPHIC_KEYS,
                    "circumstances" => CIRCUMSTANCE_KEYS,
                    "sites" => SITE_KEYS,
                    "descriptions" => DESCRIPTION_KEYS,
                    "templates" => TEMPLATE_KEYS,
                    "consequences" => CONSEQUENCE_KEYS,
                    "relations" => RELATION_KEYS,
                    "bridges" => BRIDGE_KEYS,
                    "dialogue_variants" => DIALOGUE_VARIANT_KEYS,
                    _ => unreachable!(),
                };
                keys(item, allowed, &at)?;
                let item_id = string(item, "id", &at)?;
                id(item_id, &format!("{at}.id"))?;
                if !namespaces
                    .entry(root_key)
                    .or_default()
                    .insert(item_id.into())
                {
                    return Err(format!("{at}.id: duplicate {root_key} ID {item_id}"));
                }
                match *root_key {
                    "monsters" => {
                        for key in ["name", "singular", "plural"] {
                            nonempty_string(item, key, &at)?;
                        }
                        for alias in list_strings(item, "aliases", &at)? {
                            if alias.trim().is_empty() {
                                return Err(format!("{at}.aliases: empty alias"));
                            }
                        }
                        unsigned(item, "base_weight", 1, u16::MAX.into(), &at)?;
                        unsigned(item, "curation_weight", 1, u16::MAX.into(), &at)?;
                        unsigned(item, "northern_germany_prior", 1, u16::MAX.into(), &at)?;
                        let primary_category = string(item, "primary_category", &at)?;
                        enum_value(
                            primary_category,
                            BESTIARY_CATEGORY_IDS,
                            &format!("{at}.primary_category"),
                        )?;
                        let categories = list_strings(item, "secondary_categories", &at)?;
                        let mut unique_categories = BTreeSet::new();
                        for category in categories {
                            enum_value(
                                &category,
                                BESTIARY_CATEGORY_IDS,
                                &format!("{at}.secondary_categories"),
                            )?;
                            if category == primary_category
                                || !unique_categories.insert(category.clone())
                            {
                                return Err(format!(
                                    "{at}: duplicate Bestiary category {category}"
                                ));
                            }
                        }
                        let combat = object(
                            item.get("combat")
                                .ok_or_else(|| format!("{at}.combat: missing"))?,
                            &format!("{at}.combat"),
                        )?;
                        if let Some(negotiation) = item.get("negotiation") {
                            let negotiation = object(negotiation, &format!("{at}.negotiation"))?;
                            keys(negotiation, NEGOTIATION_KEYS, &format!("{at}.negotiation"))?;
                            let sapient =
                                boolean(negotiation, "sapient", &format!("{at}.negotiation"))?;
                            let negotiable =
                                boolean(negotiation, "negotiable", &format!("{at}.negotiation"))?;
                            if negotiable && !sapient {
                                return Err(format!(
                                    "{at}.negotiation: negotiable threats must be sapient"
                                ));
                            }
                        }
                        keys(combat, COMBAT_KEYS, &format!("{at}.combat"))?;
                        enum_value(
                            string(combat, "rig", &at)?,
                            &["humanoid", "quadruped"],
                            &format!("{at}.combat.rig"),
                        )?;
                        enum_value(
                            string(combat, "attack", &at)?,
                            &[
                                "blade", "blunt", "knife", "spear", "bow", "bite", "claw", "tusk",
                            ],
                            &format!("{at}.combat.attack"),
                        )?;
                        enum_value(
                            string(combat, "protection", &at)?,
                            &[
                                "unarmored",
                                "hide",
                                "shielded",
                                "armored",
                                "bone",
                                "supernatural",
                            ],
                            &format!("{at}.combat.protection"),
                        )?;
                        enum_value(
                            string(combat, "temperament", &at)?,
                            &[
                                "cowardly",
                                "cautious",
                                "disciplined",
                                "aggressive",
                                "relentless",
                                "elusive",
                            ],
                            &format!("{at}.combat.temperament"),
                        )?;
                        unsigned(
                            combat,
                            "speed_m_per_minute",
                            1,
                            u32::MAX.into(),
                            &format!("{at}.combat"),
                        )?;
                        let weight = combat
                            .get("weight_kg")
                            .and_then(Value::as_f64)
                            .ok_or_else(|| format!("{at}.combat.weight_kg: expected number"))?;
                        if !weight.is_finite() || weight <= 0.0 || weight > f64::from(f32::MAX) {
                            return Err(format!(
                                "{at}.combat.weight_kg: must be a positive finite f32"
                            ));
                        }
                        let ranged = boolean(combat, "ranged", &format!("{at}.combat"))?;
                        let attack = string(combat, "attack", &at)?;
                        if (attack == "bow") != ranged {
                            return Err(format!(
                                "{at}.combat: bow attack and ranged flag must agree"
                            ));
                        }
                        signed(
                            combat,
                            "precision_bonus_milli",
                            i32::MIN.into(),
                            i32::MAX.into(),
                            &format!("{at}.combat"),
                        )?;
                        unsigned(
                            combat,
                            "training_multiplier_milli",
                            1,
                            u16::MAX.into(),
                            &format!("{at}.combat"),
                        )?;
                        for key in ["perception", "stealth", "morale", "disease_risk", "fear"] {
                            unsigned(combat, key, 0, 100, &format!("{at}.combat"))?;
                        }
                        let resistance = unsigned(
                            combat,
                            "resistance_joules",
                            0,
                            20_000,
                            &format!("{at}.combat"),
                        )?;
                        let padding =
                            unsigned(combat, "padding_joules", 0, 20_000, &format!("{at}.combat"))?;
                        unsigned(
                            combat,
                            "encounter_scale_basis_points",
                            1,
                            u16::MAX.into(),
                            &format!("{at}.combat"),
                        )?;
                        if let Some(loot) =
                            optional_string(combat, "loot_item_id", &format!("{at}.combat"))?
                        {
                            id(loot, &format!("{at}.combat.loot_item_id"))?;
                        }
                        if string(combat, "protection", &at)? == "armored"
                            && (resistance > 0 || padding > 0)
                        {
                            return Err(format!(
                                "{at}.combat: worn armor cannot compose with innate resistance/padding yet"
                            ));
                        }
                        let investigation = object(
                            item.get("investigation")
                                .ok_or_else(|| format!("{at}.investigation: missing"))?,
                            &format!("{at}.investigation"),
                        )?;
                        keys(
                            investigation,
                            INVESTIGATION_KEYS,
                            &format!("{at}.investigation"),
                        )?;
                        enum_value(
                            string(investigation, "activity", &at)?,
                            &["day", "night", "any"],
                            &format!("{at}.investigation.activity"),
                        )?;
                        let habitats = list_strings(investigation, "habitats", &at)?;
                        if habitats.is_empty() {
                            return Err(format!("{at}.investigation.habitats: must not be empty"));
                        }
                        for habitat in habitats {
                            enum_value(
                                &habitat,
                                &[
                                    "road",
                                    "open",
                                    "sparse_woods",
                                    "deep_woods",
                                    "cave",
                                    "crypt",
                                    "ruin",
                                    "camp",
                                    "mine",
                                    "graveyard",
                                    "occupied_house",
                                ],
                                &format!("{at}.investigation.habitats"),
                            )?;
                        }
                        for other in list_strings(investigation, "mistaken_for", &at)? {
                            monster_refs.push((at.clone(), other));
                        }
                        for key in [
                            "tracks",
                            "wounds",
                            "disturbances",
                            "odors",
                            "distinguishing_clues",
                        ] {
                            for evidence_id in list_strings(investigation, key, &at)? {
                                id(&evidence_id, &format!("{at}.investigation.{key}"))?;
                            }
                        }
                        let silhouettes = list_strings(investigation, "silhouettes", &at)?;
                        if silhouettes.is_empty() {
                            return Err(format!(
                                "{at}.investigation.silhouettes: must not be empty"
                            ));
                        }
                        for description in silhouettes {
                            description_support.push((item_id.to_owned(), description, at.clone()));
                        }
                        if list_strings(investigation, "distinguishing_clues", &at)?.is_empty() {
                            return Err(format!(
                                "{at}.investigation.distinguishing_clues: must not be empty"
                            ));
                        }
                        for hypothesis in
                            list_strings(investigation, "countermeasure_hypotheses", &at)?
                        {
                            enum_value(
                                &hypothesis,
                                &["shattering_blow", "fire", "silver", "daylight", "courage"],
                                &format!("{at}.investigation.countermeasure_hypotheses"),
                            )?;
                        }
                        for key in ["victim_tags", "sounds"] {
                            for value in list_strings(investigation, key, &at)? {
                                if value.trim().is_empty() {
                                    return Err(format!("{at}.investigation.{key}: empty value"));
                                }
                            }
                        }
                        nonempty_string(investigation, "preparation_advice", &at)?;
                        unsigned(
                            investigation,
                            "investigability",
                            0,
                            100,
                            &format!("{at}.investigation"),
                        )?;
                        enum_value(
                            string(combat, "escalation_mode", &at)?,
                            &["mob", "single"],
                            &format!("{at}.combat.escalation_mode"),
                        )?;
                        unsigned(
                            combat,
                            "escalation_growth_rate_bps",
                            1,
                            u64::from(BASIS_POINTS_PER_WHOLE),
                            &format!("{at}.combat"),
                        )?;
                        unsigned(
                            combat,
                            "baseline_combat_power",
                            u64::from(crate::threat_escalation_limits::MIN_BASELINE_ENEMY_POWER),
                            u64::from(crate::threat_escalation_limits::MAX_ORC_EQUIVALENT_POWER),
                            &format!("{at}.combat"),
                        )?;
                        boolean(
                            investigation,
                            "identification_challenge",
                            &format!("{at}.investigation"),
                        )?;
                        boolean(
                            investigation,
                            "location_challenge",
                            &format!("{at}.investigation"),
                        )?;
                    }
                    "evidence" => {
                        for key in ["portrait_label", "portrait_icon", "base_description"] {
                            nonempty_string(item, key, &at)?;
                        }
                        let topics = array(
                            item.get("topics")
                                .ok_or_else(|| format!("{at}.topics: missing"))?,
                            &format!("{at}.topics"),
                        )?;
                        if topics.is_empty() {
                            return Err(format!("{at}.topics: must not be empty"));
                        }
                        let mut topic_ids = BTreeSet::new();
                        for (topic_index, topic) in topics.iter().enumerate() {
                            let topic_at = format!("{at}.topics[{topic_index}]");
                            let topic = object(topic, &topic_at)?;
                            keys(topic, TOPIC_KEYS, &topic_at)?;
                            let topic_id = string(topic, "id", &topic_at)?;
                            id(topic_id, &format!("{topic_at}.id"))?;
                            if !topic_ids.insert(topic_id) {
                                return Err(format!("{topic_at}.id: duplicate topic"));
                            }
                            nonempty_string(topic, "label", &topic_at)?;
                            nonempty_string(topic, "inspection_description", &topic_at)?;
                            if let Some(check) = topic.get("check").filter(|value| !value.is_null())
                            {
                                let check = object(check, &format!("{topic_at}.check"))?;
                                keys(check, CHECK_KEYS, &format!("{topic_at}.check"))?;
                                enum_value(
                                    string(check, "stat", &topic_at)?,
                                    &["eyesight", "intelligence", "instinct"],
                                    &format!("{topic_at}.check.stat"),
                                )?;
                                let min = check.get("difficulty_min_milli").and_then(Value::as_u64).ok_or_else(|| format!("{topic_at}.check.difficulty_min_milli: expected integer"))?;
                                let max = check.get("difficulty_max_milli").and_then(Value::as_u64).ok_or_else(|| format!("{topic_at}.check.difficulty_max_milli: expected integer"))?;
                                if min > max || max > 10_000 {
                                    return Err(format!("{topic_at}.check: invalid DC range"));
                                }
                                nonempty_string(check, "success_description", &topic_at)?;
                                boolean(check, "reveals_clue", &format!("{topic_at}.check"))?;
                            }
                            let implications = match topic.get("bestiary") {
                                None => &[][..],
                                Some(value) => array(value, &format!("{topic_at}.bestiary"))?,
                            };
                            let mut categories = BTreeSet::new();
                            for (implication_index, implication) in implications.iter().enumerate()
                            {
                                let implication_at =
                                    format!("{topic_at}.bestiary[{implication_index}]");
                                let implication = object(implication, &implication_at)?;
                                keys(implication, BESTIARY_IMPLICATION_KEYS, &implication_at)?;
                                let category = string(implication, "category", &implication_at)?;
                                enum_value(
                                    category,
                                    BESTIARY_CATEGORY_IDS,
                                    &format!("{implication_at}.category"),
                                )?;
                                if !categories.insert(category) {
                                    return Err(format!(
                                        "{topic_at}.bestiary: duplicate category {category}"
                                    ));
                                }
                                unsigned(
                                    implication,
                                    "support_bps",
                                    0,
                                    u64::from(BASIS_POINTS_PER_WHOLE),
                                    &implication_at,
                                )?;
                                unsigned(
                                    implication,
                                    "lore_difficulty_milli",
                                    0,
                                    5_000,
                                    &implication_at,
                                )?;
                                if let Some(kind) = implication.get("diagnostic_kind") {
                                    let kind = kind.as_str().ok_or_else(|| {
                                        format!("{implication_at}.diagnostic_kind: expected string")
                                    })?;
                                    id(kind, &format!("{implication_at}.diagnostic_kind"))?;
                                }
                                let interpretation = nonempty_string(
                                    implication,
                                    "interpretation",
                                    &implication_at,
                                )?;
                                if interpretation.len() > MAX_BESTIARY_INTERPRETATION_BYTES {
                                    return Err(format!(
                                        "{implication_at}.interpretation: exceeds {MAX_BESTIARY_INTERPRETATION_BYTES} UTF-8 bytes"
                                    ));
                                }
                            }
                        }
                    }
                    "witness_demographics" => {
                        nonempty_string(item, "label", &at)?;
                        let rules = array(
                            item.get("match_rules")
                                .ok_or_else(|| format!("{at}.match_rules: missing"))?,
                            &format!("{at}.match_rules"),
                        )?;
                        if rules.is_empty() {
                            return Err(format!("{at}.match_rules: must not be empty"));
                        }
                        for (rule_index, rule) in rules.iter().enumerate() {
                            let rule_at = format!("{at}.match_rules[{rule_index}]");
                            let rule = object(rule, &rule_at)?;
                            keys(rule, MATCH_RULE_KEYS, &rule_at)?;
                            let fallback = match rule.get("fallback") {
                                None => false,
                                Some(_) => boolean(rule, "fallback", &rule_at)?,
                            };
                            if fallback {
                                fallback_demographics += 1;
                            }
                            let priority = signed(
                                rule,
                                "priority",
                                i32::MIN.into(),
                                i32::MAX.into(),
                                &rule_at,
                            )? as i32;
                            let age_bands = list_strings(rule, "age_bands", &rule_at)?;
                            let sexes = list_strings(rule, "sexes", &rule_at)?;
                            let professions = list_strings(rule, "professions", &rule_at)?;
                            let local_roles = list_strings(rule, "local_roles", &rule_at)?;
                            for age in &age_bands {
                                enum_value(
                                    age,
                                    &["child", "adolescent", "adult", "elder"],
                                    &format!("{rule_at}.age_bands"),
                                )?;
                            }
                            for sex in &sexes {
                                enum_value(sex, &["female", "male"], &format!("{rule_at}.sexes"))?;
                            }
                            for (key, values) in
                                [("professions", &professions), ("local_roles", &local_roles)]
                            {
                                for value in values {
                                    id(value, &format!("{rule_at}.{key}"))?;
                                }
                            }
                            for selector in &professions {
                                if !PROFESSION_FACTS
                                    .iter()
                                    .any(|fact| selector_matches_fact(selector, fact))
                                {
                                    return Err(format!(
                                        "{rule_at}.professions: selector {selector:?} matches no authoritative NPC profession"
                                    ));
                                }
                            }
                            for selector in &local_roles {
                                if !LOCAL_ROLE_FACTS
                                    .iter()
                                    .any(|fact| selector_matches_fact(selector, fact))
                                {
                                    return Err(format!(
                                        "{rule_at}.local_roles: selector {selector:?} matches no authoritative NPC local role"
                                    ));
                                }
                            }
                            if fallback
                                && (!age_bands.is_empty()
                                    || !sexes.is_empty()
                                    || !professions.is_empty()
                                    || !local_roles.is_empty())
                            {
                                return Err(format!(
                                    "{rule_at}: fallback rule cannot also have selectors"
                                ));
                            }
                            if !fallback
                                && age_bands.is_empty()
                                && sexes.is_empty()
                                && professions.is_empty()
                                && local_roles.is_empty()
                            {
                                return Err(format!(
                                    "{rule_at}: non-fallback rule needs a selector"
                                ));
                            }
                            demographic_rules.push(DemographicRule {
                                demographic: item_id.to_owned(),
                                at: rule_at,
                                priority,
                                age_bands,
                                sexes,
                                professions,
                                local_roles,
                                fallback,
                            });
                        }
                    }
                    "sites" => {
                        nonempty_string(item, "label", &at)?;
                        enum_value(
                            string(item, "terrain", &at)?,
                            &["underground", "forest", "settlement", "road"],
                            &format!("{at}.terrain"),
                        )?;
                        enum_value(
                            string(item, "habitat", &at)?,
                            &[
                                "road",
                                "open",
                                "sparse_woods",
                                "deep_woods",
                                "cave",
                                "crypt",
                                "ruin",
                                "camp",
                                "mine",
                                "graveyard",
                                "occupied_house",
                            ],
                            &format!("{at}.habitat"),
                        )?;
                    }
                    "circumstances" => {
                        nonempty_string(item, "statement", &at)?;
                    }
                    "descriptions" => {
                        nonempty_string(item, "text", &at)?;
                    }
                    "templates" => {
                        nonempty_string(item, "label", &at)?;
                        let routes = list_strings(item, "routes", &at)?;
                        let objectives = list_strings(item, "objectives", &at)?;
                        let (supported_routes, supported_objectives): (&[&str], &[&str]) =
                            match item_id {
                                "recurring_depredation" => (
                                    &["physical_trail", "pattern_surveillance", "social_inquiry"],
                                    &["defeat", "drive_off"],
                                ),
                                "disappearance_or_loss" => (
                                    &["physical_trail", "social_inquiry"],
                                    &["rescue", "retrieve_return", "expose"],
                                ),
                                "outbreak" => {
                                    (&["physical_trail", "social_inquiry"], &["remediate_source"])
                                }
                                _ => return Err(format!("{at}.id: no typed graph assembler")),
                            };
                        if routes.iter().map(String::as_str).collect::<Vec<_>>() != supported_routes
                            || objectives.iter().map(String::as_str).collect::<Vec<_>>()
                                != supported_objectives
                        {
                            return Err(format!("{at}: unsupported route/objective graph"));
                        }
                        unsigned(item, "incident_interval_minutes", 1, u64::MAX, &at)?;
                        unsigned(item, "maximum_incidents", 1, u8::MAX.into(), &at)?;
                        let plans = object(
                            item.get("cause_finales")
                                .ok_or_else(|| format!("{at}.cause_finales: missing"))?,
                            &format!("{at}.cause_finales"),
                        )?;
                        if plans.is_empty() {
                            return Err(format!("{at}.cause_finales: empty"));
                        }
                        let mut parsed_plans = BTreeMap::<String, Vec<String>>::new();
                        for (cause, finales) in plans {
                            if cause != "*" {
                                id(cause, &format!("{at}.cause_finales.{cause}"))?;
                            }
                            let mut parsed_finales = Vec::new();
                            for finale in array(finales, &format!("{at}.cause_finales.{cause}"))? {
                                let finale = finale.as_str().ok_or_else(|| {
                                    format!("{at}.cause_finales.{cause}: expected strings")
                                })?;
                                if !objectives.iter().any(|value| value == finale) {
                                    return Err(format!(
                                        "{at}.cause_finales.{cause}: unknown objective {finale}"
                                    ));
                                }
                                parsed_finales.push(finale.to_owned());
                            }
                            parsed_plans.insert(cause.to_owned(), parsed_finales);
                        }
                        let expected_plans = match item_id {
                            "recurring_depredation" => BTreeMap::from([(
                                "*".to_owned(),
                                vec!["defeat".to_owned(), "drive_off".to_owned()],
                            )]),
                            "disappearance_or_loss" => BTreeMap::from([
                                ("concealment".to_owned(), vec!["rescue".to_owned()]),
                                ("fabricated".to_owned(), vec!["expose".to_owned()]),
                                ("hostile".to_owned(), vec!["rescue".to_owned()]),
                                (
                                    "incidental_loss".to_owned(),
                                    vec!["retrieve_return".to_owned()],
                                ),
                            ]),
                            "outbreak" => BTreeMap::from([(
                                "*".to_owned(),
                                vec!["remediate_source".to_owned()],
                            )]),
                            _ => unreachable!(),
                        };
                        if parsed_plans != expected_plans {
                            return Err(format!(
                                "{at}.cause_finales: unsupported cause/finale coverage"
                            ));
                        }
                        template_profiles.push((
                            at.clone(),
                            item_id.to_owned(),
                            string(item, "consequence_profile", &at)?.to_owned(),
                        ));
                    }
                    "consequences" => {
                        enum_value(
                            string(item, "symptom", &at)?,
                            &[
                                "night_screams",
                                "vanished_livestock",
                                "missing_caravans",
                                "empty_stalls",
                                "sick_locals",
                            ],
                            &format!("{at}.symptom"),
                        )?;
                        if let Some(archetype) = optional_string(item, "encounter_archetype", &at)?
                        {
                            enum_value(
                                archetype,
                                &["undead", "goblins", "bandits"],
                                &format!("{at}.encounter_archetype"),
                            )?;
                        }
                        let causes = list_strings(item, "causes", &at)?;
                        if causes.is_empty() {
                            return Err(format!("{at}.causes: empty"));
                        }
                        let family = string(item, "family", &at)?;
                        id(family, &format!("{at}.family"))?;
                        for cause in &causes {
                            if cause != "*" {
                                id(cause, &format!("{at}.causes"))?;
                            }
                        }
                        consequence_families
                            .entry(family.to_owned())
                            .or_default()
                            .extend(causes);
                        signed(
                            item,
                            "buy_bps",
                            -i64::from(BASIS_POINTS_PER_WHOLE),
                            i64::from(BASIS_POINTS_PER_WHOLE),
                            &at,
                        )?;
                        signed(
                            item,
                            "sell_penalty_bps",
                            -i64::from(BASIS_POINTS_PER_WHOLE),
                            i64::from(BASIS_POINTS_PER_WHOLE),
                            &at,
                        )?;
                        unsigned(
                            item,
                            "encounter_frequency_bps",
                            0,
                            u64::from(BASIS_POINTS_PER_WHOLE),
                            &at,
                        )?;
                        unsigned(
                            item,
                            "disease_intensity",
                            0,
                            u64::from(BASIS_POINTS_PER_WHOLE),
                            &at,
                        )?;
                        nonempty_string(item, "public_summary", &at)?;
                    }
                    "bridges" => {
                        for key in ["event_suffix", "evidence_id"] {
                            id(string(item, key, &at)?, &format!("{at}.{key}"))?;
                        }
                        let action_ids = object(
                            item.get("action_ids")
                                .ok_or_else(|| format!("{at}.action_ids: missing"))?,
                            &format!("{at}.action_ids"),
                        )?;
                        keys(
                            action_ids,
                            &["recurring_depredation", "disappearance_or_loss"],
                            &format!("{at}.action_ids"),
                        )?;
                        for family in ["recurring_depredation", "disappearance_or_loss"] {
                            let action = string(action_ids, family, &format!("{at}.action_ids"))?;
                            id(action, &format!("{at}.action_ids.{family}"))?;
                            let supported = match family {
                                "recurring_depredation" => &[
                                    "locate_contact",
                                    "approach",
                                    "search",
                                    "follow",
                                    "inspect_finale",
                                    "watch",
                                    "patrol",
                                    "reveal_route",
                                    "ambush",
                                ][..],
                                "disappearance_or_loss" => &[
                                    "inspect_last_known",
                                    "resolve_physical",
                                    "follow",
                                    "locate_contact",
                                    "approach_social",
                                    "resolve_social",
                                ][..],
                                _ => unreachable!(),
                            };
                            if !supported.contains(&action) {
                                return Err(format!(
                                    "{at}.action_ids.{family}: action {action} is not emitted by the typed assembler"
                                ));
                            }
                        }
                        if string(item, "explanation", &at)?.trim().is_empty()
                            || string(item, "lead_summary", &at)?.trim().is_empty()
                        {
                            return Err(format!("{at}: bridge prose must not be empty"));
                        }
                    }
                    "dialogue_variants" => {
                        enum_value(
                            string(item, "kind", &at)?,
                            &["referral"],
                            &format!("{at}.kind"),
                        )?;
                        signed(item, "priority", -10_000, 10_000, &at)?;
                        let template = nonempty_string(item, "template", &at)?;
                        if template.chars().count() > MAX_QUEST_DIALOGUE_TEMPLATE_CHARS {
                            return Err(format!("{at}.template: too long"));
                        }
                        if let Some(condition) = item.get("conditions") {
                            validate_dialogue_condition(condition, &format!("{at}.conditions"))?;
                        }
                    }
                    "relations" => {
                        let candidates = array(
                            item.get("candidates")
                                .ok_or_else(|| format!("{at}.candidates: missing"))?,
                            &format!("{at}.candidates"),
                        )?;
                        if candidates.is_empty() {
                            return Err(format!("{at}.candidates: empty"));
                        }
                        let mut seen = BTreeSet::new();
                        let mut parsed = Vec::new();
                        for (candidate_index, candidate) in candidates.iter().enumerate() {
                            let candidate_at = format!("{at}.candidates[{candidate_index}]");
                            let candidate = object(candidate, &candidate_at)?;
                            keys(candidate, CANDIDATE_KEYS, &candidate_at)?;
                            let candidate_id = string(candidate, "id", &candidate_at)?;
                            id(candidate_id, &format!("{candidate_at}.id"))?;
                            if !seen.insert(candidate_id) {
                                return Err(format!("{candidate_at}.id: duplicate candidate"));
                            }
                            let p = unsigned(
                                candidate,
                                "plausibility",
                                0,
                                u32::MAX.into(),
                                &candidate_at,
                            )?;
                            let c =
                                unsigned(candidate, "curation", 0, u32::MAX.into(), &candidate_at)?;
                            let zero_reason =
                                optional_string(candidate, "hard_zero_reason", &candidate_at)?
                                    .map(str::to_owned);
                            if zero_reason
                                .as_ref()
                                .is_some_and(|reason| reason.trim().is_empty())
                            {
                                return Err(format!(
                                    "{candidate_at}.hard_zero_reason: must not be empty"
                                ));
                            }
                            if (p == 0 || c == 0) != zero_reason.is_some() {
                                return Err(format!("{candidate_at}: zero weight/reason mismatch"));
                            }
                            let bridge =
                                optional_string(candidate, "required_bridge", &candidate_at)?
                                    .map(str::to_owned);
                            if let Some(value) = &bridge {
                                id(value, &format!("{candidate_at}.required_bridge"))?;
                                bridge_refs.push((candidate_at.clone(), value.clone()));
                            }
                            for factor in
                                optional_list_strings(candidate, "factors", &candidate_at)?
                            {
                                id(&factor, &format!("{candidate_at}.factors"))?;
                            }
                            parsed.push((candidate_id.into(), p, c, zero_reason, bridge));
                        }
                        relations.insert(item_id.into(), parsed);
                    }
                    _ => {}
                }
            }
        }
    }
    let monsters = namespaces.get("monsters").cloned().unwrap_or_default();
    let evidence = namespaces.get("evidence").cloned().unwrap_or_default();
    let demographics = namespaces
        .get("witness_demographics")
        .cloned()
        .unwrap_or_default();
    let circumstances = namespaces.get("circumstances").cloned().unwrap_or_default();
    let sites = namespaces.get("sites").cloned().unwrap_or_default();
    let descriptions = namespaces.get("descriptions").cloned().unwrap_or_default();
    let templates = namespaces.get("templates").cloned().unwrap_or_default();
    let bridges = namespaces.get("bridges").cloned().unwrap_or_default();
    for (at, target) in monster_refs {
        if !monsters.contains(&target) {
            return Err(format!("{at}.mistaken_for: dangling monster {target}"));
        }
    }
    for (at, bridge) in bridge_refs {
        if !bridges.contains(&bridge) {
            return Err(format!("{at}.required_bridge: dangling bridge {bridge}"));
        }
    }
    for (monster, description, at) in description_support {
        if !descriptions.contains(&description) {
            return Err(format!(
                "{at}.silhouettes: dangling description {description}"
            ));
        }
        let relation = relations
            .get(&format!("description.{description}"))
            .ok_or_else(|| format!("{at}.silhouettes: missing description relation"))?;
        if !relation
            .iter()
            .any(|candidate| candidate.0 == monster && candidate.1 > 0 && candidate.2 > 0)
        {
            return Err(format!(
                "{at}.silhouettes: missing positive likelihood for {monster}/{description}"
            ));
        }
    }
    for (relation_id, candidates) in &relations {
        let (namespace, context) = relation_id
            .split_once('.')
            .map_or((relation_id.as_str(), None), |(left, right)| {
                (left, Some(right))
            });
        let allowed: Option<&BTreeSet<String>> = match namespace {
            "family" if context.is_none() => Some(&templates),
            "cause" => {
                let template = context.ok_or_else(|| {
                    format!("catalog.relations.{relation_id}: missing template suffix")
                })?;
                if !templates.contains(template) {
                    return Err(format!(
                        "catalog.relations.{relation_id}: dangling template {template}"
                    ));
                }
                None
            }
            "disease" => {
                let template = context.ok_or_else(|| {
                    format!("catalog.relations.{relation_id}: missing template suffix")
                })?;
                if template != "outbreak" {
                    return Err(format!(
                        "catalog.relations.{relation_id}: disease relations are supported only for outbreak"
                    ));
                }
                None
            }
            "source" => {
                let disease = context.ok_or_else(|| {
                    format!("catalog.relations.{relation_id}: missing disease suffix")
                })?;
                if ![
                    "influenza",
                    "mahrdruck",
                    "shroud_fever",
                    "bilwisschuss",
                    "kobeldunst",
                ]
                .contains(&disease)
                {
                    return Err(format!(
                        "catalog.relations.{relation_id}: dangling outbreak disease {disease}"
                    ));
                }
                None
            }
            "site" => {
                let monster = context.ok_or_else(|| {
                    format!("catalog.relations.{relation_id}: missing monster suffix")
                })?;
                if !monsters.contains(monster) {
                    return Err(format!(
                        "catalog.relations.{relation_id}: dangling monster {monster}"
                    ));
                }
                Some(&sites)
            }
            "circumstance" => {
                let demographic = context.ok_or_else(|| {
                    format!("catalog.relations.{relation_id}: missing demographic suffix")
                })?;
                if !demographics.contains(demographic) {
                    return Err(format!(
                        "catalog.relations.{relation_id}: dangling demographic {demographic}"
                    ));
                }
                Some(&circumstances)
            }
            "description" => {
                let description = context.ok_or_else(|| {
                    format!("catalog.relations.{relation_id}: missing description suffix")
                })?;
                if !descriptions.contains(description) {
                    return Err(format!(
                        "catalog.relations.{relation_id}: dangling description {description}"
                    ));
                }
                Some(&monsters)
            }
            "evidence" => Some(&evidence),
            "reliability" | "account" | "route" | "pattern" => None,
            _ => {
                return Err(format!(
                    "catalog.relations.{relation_id}: unsupported relation namespace"
                ));
            }
        };
        for (candidate, _, _, _, required_bridge) in candidates {
            if let Some(allowed) = allowed
                && !allowed.contains(candidate)
            {
                return Err(format!(
                    "catalog.relations.{relation_id}: dangling candidate {candidate}"
                ));
            }
            let closed = match namespace {
                "cause" if context == Some("outbreak") => {
                    Some(&["sanitation", "behavior", "threat_vector", "environmental"][..])
                }
                "cause" if !monsters.contains(candidate) => {
                    Some(&["concealment", "incidental_loss", "fabricated"][..])
                }
                "disease" => Some(
                    &[
                        "influenza",
                        "mahrdruck",
                        "shroud_fever",
                        "bilwisschuss",
                        "kobeldunst",
                    ][..],
                ),
                "source" => Some(&["sanitation", "behavior", "threat_vector", "environmental"][..]),
                "reliability" => Some(
                    &[
                        "truthful",
                        "mistaken",
                        "evasive",
                        "deceptive",
                        "partly_truthful",
                    ][..],
                ),
                "account" => Some(&["visual", "heard", "tracks"][..]),
                "route" => Some(&["direct", "cautious"][..]),
                "pattern" => Some(&["nightly", "roadside", "victim_specific", "irregular"][..]),
                _ => None,
            };
            if let Some(closed) = closed
                && !closed.contains(&candidate.as_str())
            {
                return Err(format!(
                    "catalog.relations.{relation_id}: unknown candidate {candidate}"
                ));
            }
            if namespace == "evidence" && required_bridge.is_some() {
                return Err(format!(
                    "catalog.relations.{relation_id}: evidence relations cannot require bridges because follow-up evidence selection has no bridge materialization context"
                ));
            }
        }
    }
    if fallback_demographics != 1 {
        return Err(format!(
            "catalog.witness_demographics: expected exactly one fallback rule, found {fallback_demographics}"
        ));
    }
    for (index, left) in demographic_rules.iter().enumerate() {
        for right in demographic_rules.iter().skip(index + 1) {
            if left.demographic != right.demographic
                && !left.fallback
                && !right.fallback
                && left.priority == right.priority
                && selectors_overlap(
                    &left.age_bands,
                    &right.age_bands,
                    &["child", "adolescent", "adult", "elder"],
                )
                && selectors_overlap(&left.sexes, &right.sexes, &["female", "male"])
                && selectors_overlap(&left.professions, &right.professions, PROFESSION_FACTS)
                && selectors_overlap(&left.local_roles, &right.local_roles, LOCAL_ROLE_FACTS)
            {
                return Err(format!(
                    "{} and {}: equal-priority demographic rules can match the same NPC",
                    left.at, right.at
                ));
            }
        }
    }
    for (at, template_id, profile) in template_profiles {
        let causes = consequence_families.get(&profile).ok_or_else(|| {
            format!("{at}.consequence_profile: missing consequence family {profile}")
        })?;
        if !causes.contains("*") {
            let cause_relation = relations
                .get(&format!("cause.{template_id}"))
                .ok_or_else(|| format!("{at}: missing cause relation"))?;
            for (cause, plausibility, curation, _, _) in cause_relation {
                if *plausibility > 0 && *curation > 0 && !causes.contains(cause) {
                    return Err(format!(
                        "{at}.consequence_profile: no consequence for possible cause {cause}"
                    ));
                }
            }
        }
    }
    for required in [
        "family",
        "cause.recurring_depredation",
        "cause.disappearance_or_loss",
        "cause.outbreak",
        "disease.outbreak",
        "source.influenza",
        "source.mahrdruck",
        "source.shroud_fever",
        "source.bilwisschuss",
        "source.kobeldunst",
        "reliability.baseline",
        "account.baseline",
        "route.recurring_depredation",
        "route.disappearance_or_loss",
        "route.outbreak",
        "pattern.recurring_depredation",
        "pattern.disappearance_or_loss",
        "pattern.outbreak",
        "evidence.baseline",
    ] {
        if !relations.contains_key(required) {
            return Err(format!(
                "catalog.relations: missing required relation {required}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_are_bounded_to_runtime_capacity() {
        assert!(id(&"a".repeat(63), "fixture").is_ok());
        assert!(id(&"a".repeat(64), "fixture").is_err());
    }
}
