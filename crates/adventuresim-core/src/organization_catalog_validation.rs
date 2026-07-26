use serde_json::{Map, Value};
use std::collections::BTreeSet;

const SKILLS: &[&str] = &[
    "will",
    "insight",
    "self_awareness",
    "humor",
    "command",
    "deception",
    "seduction",
    "physiology",
    "cooking",
    "religion",
    "bestiary",
    "anatomy",
    "polearm",
    "axe",
    "bludgeon",
    "sword",
    "knife",
    "bow",
    "crossbow",
    "firearm",
    "throw",
    "block",
    "dodge",
    "stealth",
    "balance",
    "terrain_plains",
    "terrain_forest",
    "terrain_hills",
    "terrain_urban",
    "tailoring",
    "smithing",
];
const RELIGIONS: &[&str] = &[
    "roman_catholic",
    "lutheran",
    "reformed",
    "anglican",
    "eastern_orthodox",
    "islamic",
    "judaism",
];
const BESTIARY: &[&str] = &[
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
const TERRAINS: &[&str] = &["plains", "forest", "hills", "urban"];

fn object<'a>(
    value: &'a Value,
    source: &str,
    path: &str,
) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{source}: {path} must be an object"))
}

fn text<'a>(object: &'a Map<String, Value>, source: &str, field: &str) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{source}: {field} must be a non-empty string"))
}

fn array<'a>(
    object: &'a Map<String, Value>,
    source: &str,
    field: &str,
) -> Result<&'a Vec<Value>, String> {
    object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{source}: {field} must be an array"))
}

fn keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    source: &str,
    path: &str,
) -> Result<(), String> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("{source}: {path} contains unknown field {key:?}"));
        }
    }
    Ok(())
}

fn stable_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn unique_strings(values: &[Value], source: &str, path: &str) -> Result<BTreeSet<String>, String> {
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let Some(value) = value.as_str().filter(|value| !value.is_empty()) else {
            return Err(format!(
                "{source}: {path}[{index}] must be a non-empty string"
            ));
        };
        if !seen.insert(value.to_owned()) {
            return Err(format!("{source}: {path} contains duplicate {value:?}"));
        }
    }
    Ok(seen)
}

fn requirements(values: &[Value], source: &str, path: &str) -> Result<(), String> {
    for (index, value) in values.iter().enumerate() {
        let at = format!("{path}[{index}]");
        let req = object(value, source, &at)?;
        match text(req, source, "kind")? {
            "skill_rating" => {
                keys(req, &["kind", "skill", "minimum", "leaf"], source, &at)?;
                let skill = text(req, source, "skill")?;
                if !SKILLS.contains(&skill) {
                    return Err(format!(
                        "{source}: {at}.skill references unknown skill {skill:?}"
                    ));
                }
                let minimum = req
                    .get("minimum")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("{source}: {at}.minimum must be a number"))?;
                if !(0.0..=5.0).contains(&minimum) {
                    return Err(format!("{source}: {at}.minimum must be between 0 and 5"));
                }
                let leaf = req.get("leaf");
                match skill {
                    "religion" => {
                        let leaf = leaf.and_then(Value::as_str).ok_or_else(|| {
                            format!("{source}: {at}.leaf is required for religion")
                        })?;
                        if !RELIGIONS.contains(&leaf) {
                            return Err(format!("{source}: {at}.leaf is invalid for religion"));
                        }
                    }
                    "bestiary" => {
                        let leaf = leaf.and_then(Value::as_str).ok_or_else(|| {
                            format!("{source}: {at}.leaf is required for bestiary")
                        })?;
                        if !BESTIARY.contains(&leaf) {
                            return Err(format!("{source}: {at}.leaf is invalid for bestiary"));
                        }
                    }
                    _ if leaf.is_some() => {
                        return Err(format!("{source}: {at}.leaf is forbidden for {skill}"));
                    }
                    _ => {}
                }
            }
            "professed_religion" => {
                keys(req, &["kind", "religion"], source, &at)?;
                let religion = text(req, source, "religion")?;
                if !RELIGIONS.contains(&religion) {
                    return Err(format!(
                        "{source}: {at}.religion references unknown religion {religion:?}"
                    ));
                }
            }
            kind => {
                return Err(format!(
                    "{source}: {at}.kind has unknown requirement {kind:?}"
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_documents(
    documents: &[Value],
    sources: &[String],
    policy: &Value,
    policy_source: &str,
) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for (document, source) in documents.iter().zip(sources) {
        let org = object(document, source, "organization")?;
        keys(
            org,
            &[
                "id",
                "name",
                "description",
                "historical_fantasy_note",
                "service_id",
                "chapters",
                "recognition",
                "admission",
                "dues",
                "ranks",
                "activity",
                "privileges",
            ],
            source,
            "organization",
        )?;
        let id = text(org, source, "id")?;
        if !stable_id(id) {
            return Err(format!(
                "{source}: organization id {id:?} is not snake_case"
            ));
        }
        if !ids.insert(id.to_owned()) {
            return Err(format!("{source}: duplicate organization id {id:?}"));
        }
        text(org, source, "name")?;
        text(org, source, "description")?;
        unique_strings(array(org, source, "chapters")?, source, "chapters")?;
        let recognition = object(
            org.get("recognition")
                .ok_or_else(|| format!("{source}: recognition is required"))?,
            source,
            "recognition",
        )?;
        match text(recognition, source, "kind")? {
            "universal" => keys(recognition, &["kind"], source, "recognition")?,
            "settlements" => {
                keys(
                    recognition,
                    &["kind", "settlement_ids"],
                    source,
                    "recognition",
                )?;
                unique_strings(
                    array(recognition, source, "settlement_ids")?,
                    source,
                    "recognition.settlement_ids",
                )?;
            }
            kind => return Err(format!("{source}: recognition.kind {kind:?} is invalid")),
        }
        if let Some(admission) = org.get("admission") {
            let admission = object(admission, source, "admission")?;
            keys(
                admission,
                &["joining_fee", "requirements"],
                source,
                "admission",
            )?;
            if admission
                .get("joining_fee")
                .is_some_and(|fee| fee.as_u64().is_none_or(|n| n > u32::MAX as u64))
            {
                return Err(format!("{source}: admission.joining_fee must be a u32"));
            }
            requirements(
                array(admission, source, "requirements")?,
                source,
                "admission.requirements",
            )?;
        }
        if let Some(dues) = org.get("dues") {
            let dues = object(dues, source, "dues")?;
            keys(dues, &["amount", "interval_days"], source, "dues")?;
            let amount = dues.get("amount").and_then(Value::as_u64).unwrap_or(0);
            let days = dues
                .get("interval_days")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if amount == 0 || amount > u32::MAX as u64 || days == 0 || days > u32::MAX as u64 {
                return Err(format!(
                    "{source}: dues amount and interval_days must be positive u32 values"
                ));
            }
        }
        let ranks = array(org, source, "ranks")?;
        if ranks.is_empty() {
            return Err(format!("{source}: ranks must not be empty"));
        }
        let mut rank_ids = BTreeSet::new();
        for (index, rank) in ranks.iter().enumerate() {
            let at = format!("ranks[{index}]");
            let rank = object(rank, source, &at)?;
            keys(
                rank,
                &[
                    "id",
                    "name",
                    "description",
                    "requirements",
                    "practice_allowed",
                    "practice_reward_interval_minutes",
                ],
                source,
                &at,
            )?;
            let rank_id = text(rank, source, "id")?;
            if !stable_id(rank_id) {
                return Err(format!("{source}: {at}.id {rank_id:?} is not snake_case"));
            }
            if !rank_ids.insert(rank_id) {
                return Err(format!("{source}: duplicate rank id {rank_id:?}"));
            }
            text(rank, source, "name")?;
            text(rank, source, "description")?;
            requirements(
                array(rank, source, "requirements")?,
                source,
                &format!("{at}.requirements"),
            )?;
            let practice_allowed = rank
                .get("practice_allowed")
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("{source}: {at}.practice_allowed must be a boolean"))?;
            let cadence = rank
                .get("practice_reward_interval_minutes")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!("{source}: {at}.practice_reward_interval_minutes must be a u32")
                })?;
            if cadence > u32::MAX as u64 || (practice_allowed && cadence == 0) {
                return Err(format!(
                    "{source}: {at}.practice_reward_interval_minutes must be positive when practice is allowed"
                ));
            }
            if !practice_allowed && cadence != 0 {
                return Err(format!(
                    "{source}: {at}.practice_reward_interval_minutes must be zero when practice is forbidden"
                ));
            }
        }
        let activity = object(
            org.get("activity")
                .ok_or_else(|| format!("{source}: activity is required"))?,
            source,
            "activity",
        )?;
        keys(activity, &["training", "reward"], source, "activity")?;
        if !["gold", "virtue"].contains(&text(activity, source, "reward")?) {
            return Err(format!("{source}: activity.reward must be gold or virtue"));
        }
        let training = array(activity, source, "training")?;
        let mut total = 0.0;
        for (index, entry) in training.iter().enumerate() {
            let at = format!("activity.training[{index}]");
            let entry = object(entry, source, &at)?;
            let kind = text(entry, source, "kind")?;
            let weight = entry
                .get("weight")
                .and_then(Value::as_f64)
                .unwrap_or(f64::NAN);
            if !weight.is_finite() || weight <= 0.0 {
                return Err(format!("{source}: {at}.weight must be positive and finite"));
            }
            total += weight;
            match kind {
                "fixed_skill" => {
                    keys(entry, &["kind", "skill", "weight"], source, &at)?;
                    let skill = text(entry, source, "skill")?;
                    if !SKILLS.contains(&skill) || ["religion", "bestiary"].contains(&skill) {
                        return Err(format!(
                            "{source}: {at}.skill references invalid fixed skill {skill:?}"
                        ));
                    }
                }
                "religion" => {
                    keys(entry, &["kind", "religion", "weight"], source, &at)?;
                    if !RELIGIONS.contains(&text(entry, source, "religion")?) {
                        return Err(format!("{source}: {at}.religion is unknown"));
                    }
                }
                "bestiary" => {
                    keys(entry, &["kind", "category", "weight"], source, &at)?;
                    if !BESTIARY.contains(&text(entry, source, "category")?) {
                        return Err(format!("{source}: {at}.category is unknown"));
                    }
                }
                "terrain" => {
                    keys(entry, &["kind", "terrain", "weight"], source, &at)?;
                    if !TERRAINS.contains(&text(entry, source, "terrain")?) {
                        return Err(format!("{source}: {at}.terrain is unknown"));
                    }
                }
                "equipped_weapon_skills" => keys(entry, &["kind", "weight"], source, &at)?,
                _ => {
                    return Err(format!(
                        "{source}: {at}.kind has unknown training target {kind:?}"
                    ));
                }
            }
        }
        if !training.is_empty() && (total - 1.0).abs() > 0.000_001 {
            return Err(format!(
                "{source}: activity training weights total {total}, expected 1"
            ));
        }
        for privilege in array(org, source, "privileges")? {
            if !matches!(privilege.as_str(), Some("bear_arms" | "wear_armor")) {
                return Err(format!("{source}: unknown privilege {privilege}"));
            }
        }
    }
    let policies = policy
        .as_array()
        .ok_or_else(|| format!("{policy_source}: root must be an array"))?;
    let mut policy_ids = BTreeSet::new();
    for (index, value) in policies.iter().enumerate() {
        let at = format!("policies[{index}]");
        let row = object(value, policy_source, &at)?;
        keys(
            row,
            &["settlement_id", "restrict_arms", "restrict_armor"],
            policy_source,
            &at,
        )?;
        let id = text(row, policy_source, "settlement_id")?;
        if !policy_ids.insert(id) {
            return Err(format!(
                "{policy_source}: duplicate settlement policy {id:?}"
            ));
        }
        for field in ["restrict_arms", "restrict_armor"] {
            if row.get(field).is_some_and(|value| !value.is_boolean()) {
                return Err(format!("{policy_source}: {at}.{field} must be a boolean"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_organization() -> Value {
        json!({
            "id": "test_body",
            "name": "Test Body",
            "description": "A validator fixture.",
            "chapters": ["viabundus-0"],
            "recognition": {"kind": "settlements", "settlement_ids": ["viabundus-0"]},
            "admission": {"joining_fee": 0, "requirements": []},
            "ranks": [{
                "id": "member",
                "name": "Member",
                "description": "A member.",
                "requirements": [],
                "practice_allowed": true,
                "practice_reward_interval_minutes": 480
            }],
            "activity": {
                "training": [{"kind": "fixed_skill", "skill": "cooking", "weight": 1.0}],
                "reward": "gold"
            },
            "privileges": []
        })
    }

    fn validate(organization: Value, policy: Value) -> Result<(), String> {
        validate_documents(
            &[organization],
            &["fixture.yaml".into()],
            &policy,
            "policies.yaml",
        )
    }

    #[test]
    fn rejects_missing_typed_leaf_and_forbidden_leaf() {
        let mut missing = valid_organization();
        missing["admission"]["requirements"] =
            json!([{"kind": "skill_rating", "skill": "religion", "minimum": 1.0}]);
        assert!(
            validate(missing, json!([]))
                .unwrap_err()
                .contains("leaf is required")
        );

        let mut forbidden = valid_organization();
        forbidden["admission"]["requirements"] = json!([
            {"kind": "skill_rating", "skill": "cooking", "leaf": "roman_catholic", "minimum": 1.0}
        ]);
        assert!(
            validate(forbidden, json!([]))
                .unwrap_err()
                .contains("leaf is forbidden")
        );
    }

    #[test]
    fn rejects_non_string_and_duplicate_settlement_ids() {
        let mut non_string = valid_organization();
        non_string["chapters"] = json!(["viabundus-0", 7]);
        assert!(
            validate(non_string, json!([]))
                .unwrap_err()
                .contains("must be a non-empty string")
        );

        let mut duplicate = valid_organization();
        duplicate["recognition"]["settlement_ids"] = json!(["viabundus-0", "viabundus-0"]);
        assert!(
            validate(duplicate, json!([]))
                .unwrap_err()
                .contains("duplicate")
        );
    }

    #[test]
    fn rejects_non_boolean_policy_flags() {
        let policy = json!([{
            "settlement_id": "viabundus-0",
            "restrict_arms": "yes",
            "restrict_armor": false
        }]);
        assert!(
            validate(valid_organization(), policy)
                .unwrap_err()
                .contains("must be a boolean")
        );
    }
}
