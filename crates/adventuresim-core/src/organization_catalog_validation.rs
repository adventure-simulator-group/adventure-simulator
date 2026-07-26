use serde_json::{Map, Value};
use std::collections::BTreeSet;

const SKILLS: &[&str] = &[
    "will",
    "insight",
    "charm",
    "command",
    "deception",
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

fn professed_religions(values: &[Value]) -> BTreeSet<&str> {
    values
        .iter()
        .filter_map(|value| {
            let requirement = value.as_object()?;
            (requirement.get("kind")?.as_str()? == "professed_religion")
                .then(|| requirement.get("religion")?.as_str())
                .flatten()
        })
        .collect()
}

pub fn validate_documents(
    documents: &[Value],
    sources: &[String],
    policy: &Value,
    policy_source: &str,
) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    let mut starting_professions = BTreeSet::new();
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
                "starting_role",
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
        if let Some(starting_role) = org.get("starting_role") {
            let starting_role = object(starting_role, source, "starting_role")?;
            keys(
                starting_role,
                &["profession", "adult_rank_id", "old_rank_id"],
                source,
                "starting_role",
            )?;
            let profession = text(starting_role, source, "profession")?;
            if ![
                "merchant",
                "weaponsmith",
                "armourer",
                "tailor",
                "herbalist",
                "cook",
                "learned_religious_practitioner",
                "witch_hunter",
                "knight",
                "forester",
            ]
            .contains(&profession)
            {
                return Err(format!(
                    "{source}: starting_role.profession {profession:?} is invalid"
                ));
            }
            starting_professions.insert(profession.to_owned());
        }
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
                    "privileges",
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
            if let Some(privileges) = rank.get("privileges") {
                let privileges = privileges
                    .as_array()
                    .ok_or_else(|| format!("{source}: {at}.privileges must be an array"))?;
                validate_privileges(privileges, source, &format!("{at}.privileges"))?;
            }
        }
        if let Some(starting_role) = org.get("starting_role").and_then(Value::as_object) {
            let adult = text(starting_role, source, "adult_rank_id")?;
            let old = text(starting_role, source, "old_rank_id")?;
            if adult == old {
                return Err(format!(
                    "{source}: starting_role adult and old ranks must be distinct"
                ));
            }
            if !rank_ids.contains(adult) || !rank_ids.contains(old) {
                return Err(format!(
                    "{source}: starting_role references a missing adult or old rank"
                ));
            }
            if array(org, source, "chapters")?.is_empty() {
                return Err(format!(
                    "{source}: starting_role organizations must have a playable chapter"
                ));
            }
            if recognition.get("kind").and_then(Value::as_str) == Some("settlements") {
                let chapters = array(org, source, "chapters")?;
                let recognized = array(recognition, source, "settlement_ids")?;
                if !chapters.iter().any(|chapter| recognized.contains(chapter)) {
                    return Err(format!(
                        "{source}: starting_role organization has no chapter in a recognized settlement"
                    ));
                }
            }
            let admission_requirements = org
                .get("admission")
                .and_then(Value::as_object)
                .and_then(|admission| admission.get("requirements"))
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            for rank_id in [adult, old] {
                let rank_requirements = ranks
                    .iter()
                    .find_map(|rank| {
                        let rank = rank.as_object()?;
                        (rank.get("id")?.as_str()? == rank_id)
                            .then(|| rank.get("requirements")?.as_array())
                            .flatten()
                    })
                    .map_or(&[][..], Vec::as_slice);
                let religions = professed_religions(admission_requirements)
                    .into_iter()
                    .chain(professed_religions(rank_requirements))
                    .collect::<BTreeSet<_>>();
                if religions.len() > 1 {
                    return Err(format!(
                        "{source}: starting_role rank {rank_id:?} conflicts with admission religion"
                    ));
                }
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
        validate_privileges(array(org, source, "privileges")?, source, "privileges")?;
    }
    for profession in [
        "merchant",
        "weaponsmith",
        "armourer",
        "tailor",
        "herbalist",
        "cook",
        "learned_religious_practitioner",
        "witch_hunter",
        "knight",
        "forester",
    ] {
        if documents.len() > 1 && !starting_professions.contains(profession) {
            return Err(format!(
                "organization catalog has no eligible starting organization for {profession}"
            ));
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

fn validate_privileges(values: &[Value], source: &str, path: &str) -> Result<(), String> {
    let privileges = unique_strings(values, source, path)?;
    for privilege in privileges {
        if !matches!(
            privilege.as_str(),
            "bear_arms"
                | "wear_armor"
                | "forage_high_game"
                | "forage_low_game"
                | "forage_fish"
                | "forage_plants"
        ) {
            return Err(format!(
                "{source}: {path} contains unknown privilege {privilege:?}"
            ));
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
                "practice_reward_interval_minutes": 480,
                "privileges": []
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

    #[test]
    fn validates_unknown_and_duplicate_privileges_at_both_levels() {
        let mut duplicate_org = valid_organization();
        duplicate_org["privileges"] = json!(["forage_fish", "forage_fish"]);
        assert!(
            validate(duplicate_org, json!([]))
                .unwrap_err()
                .contains("duplicate")
        );

        let mut unknown_rank = valid_organization();
        unknown_rank["ranks"][0]["privileges"] = json!(["royal_hunt"]);
        assert!(
            validate(unknown_rank, json!([]))
                .unwrap_err()
                .contains("unknown privilege")
        );

        let mut duplicate_rank = valid_organization();
        duplicate_rank["ranks"][0]["privileges"] = json!(["forage_high_game", "forage_high_game"]);
        assert!(
            validate(duplicate_rank, json!([]))
                .unwrap_err()
                .contains("duplicate")
        );
    }

    #[test]
    fn rejects_bad_starting_rank_mappings() {
        let mut same_rank = valid_organization();
        same_rank["starting_role"] = json!({
            "profession": "cook",
            "adult_rank_id": "member",
            "old_rank_id": "member"
        });
        assert!(
            validate(same_rank, json!([]))
                .unwrap_err()
                .contains("must be distinct")
        );

        let mut missing_rank = valid_organization();
        missing_rank["starting_role"] = json!({
            "profession": "cook",
            "adult_rank_id": "member",
            "old_rank_id": "master"
        });
        assert!(
            validate(missing_rank, json!([]))
                .unwrap_err()
                .contains("missing")
        );
    }

    #[test]
    fn rejects_starting_chapter_outside_scoped_recognition() {
        let mut organization = valid_organization();
        organization["ranks"].as_array_mut().unwrap().push(json!({
            "id": "master",
            "name": "Master",
            "description": "A master.",
            "requirements": [],
            "practice_allowed": true,
            "practice_reward_interval_minutes": 120
        }));
        organization["starting_role"] = json!({
            "profession": "cook",
            "adult_rank_id": "member",
            "old_rank_id": "master"
        });
        organization["recognition"]["settlement_ids"] = json!(["viabundus-99"]);
        assert!(
            validate(organization, json!([]))
                .unwrap_err()
                .contains("no chapter in a recognized settlement")
        );
    }

    #[test]
    fn rejects_starting_rank_religion_conflict() {
        let mut organization = valid_organization();
        organization["admission"]["requirements"] =
            json!([{"kind": "professed_religion", "religion": "lutheran"}]);
        organization["ranks"] = json!([
            {
                "id": "member",
                "name": "Member",
                "description": "A member.",
                "requirements": [{"kind": "professed_religion", "religion": "roman_catholic"}],
                "practice_allowed": true,
                "practice_reward_interval_minutes": 480
            },
            {
                "id": "master",
                "name": "Master",
                "description": "A master.",
                "requirements": [],
                "practice_allowed": true,
                "practice_reward_interval_minutes": 120
            }
        ]);
        organization["starting_role"] = json!({
            "profession": "cook",
            "adult_rank_id": "member",
            "old_rank_id": "master"
        });
        assert!(
            validate(organization, json!([]))
                .unwrap_err()
                .contains("conflicts with admission religion")
        );
    }
}
