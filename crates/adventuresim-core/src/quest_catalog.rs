//! Startup-compiled, repository-authored quest and bestiary content.
//!
//! Files under `content/quests` are sorted, validated, embedded, and hashed by
//! `build.rs`. Deployment never reads loose data files.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

include!(concat!(env!("OUT_DIR"), "/quest_catalog.rs"));

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogDocument {
    #[serde(default)]
    pub monsters: Vec<Monster>,
    #[serde(default)]
    pub evidence: Vec<EvidenceDefinition>,
    #[serde(default)]
    pub witness_demographics: Vec<WitnessDemographicDefinition>,
    #[serde(default)]
    pub circumstances: Vec<CircumstanceDefinition>,
    #[serde(default)]
    pub sites: Vec<SiteDefinition>,
    #[serde(default)]
    pub descriptions: Vec<DescriptionDefinition>,
    #[serde(default)]
    pub templates: Vec<TemplateDefinition>,
    #[serde(default)]
    pub consequences: Vec<ConsequenceDefinition>,
    #[serde(default)]
    pub relations: Vec<WeightedRelation>,
    #[serde(default)]
    pub bridges: Vec<BridgeDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Monster {
    pub id: String,
    pub name: String,
    pub singular: String,
    pub plural: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub base_weight: u16,
    pub curation_weight: u16,
    pub northern_germany_prior: u16,
    pub combat: MonsterCombat,
    pub investigation: MonsterInvestigation,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterCombat {
    pub rig: String,
    pub speed_m_per_minute: u32,
    pub weight_kg: f32,
    pub attack: String,
    pub ranged: bool,
    pub precision_bonus_milli: i32,
    pub training_multiplier_milli: u16,
    pub perception: u8,
    pub stealth: u8,
    pub morale: u8,
    pub protection: String,
    pub resistance_joules: u32,
    pub padding_joules: u32,
    pub disease_risk: u8,
    pub fear: u8,
    pub temperament: String,
    pub encounter_scale_basis_points: u16,
    pub loot_item_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterInvestigation {
    pub habitats: Vec<String>,
    pub activity: String,
    pub victim_tags: Vec<String>,
    pub tracks: Vec<String>,
    pub wounds: Vec<String>,
    pub disturbances: Vec<String>,
    pub sounds: Vec<String>,
    pub silhouettes: Vec<String>,
    pub odors: Vec<String>,
    pub mistaken_for: Vec<String>,
    pub distinguishing_clues: Vec<String>,
    pub preparation_advice: String,
    pub evidence_visibility: u8,
    pub identification_challenge: bool,
    pub location_challenge: bool,
    pub countermeasure_hypotheses: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceDefinition {
    pub id: String,
    pub portrait_label: String,
    pub portrait_icon: String,
    pub base_description: String,
    pub topics: Vec<EvidenceTopicDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceTopicDefinition {
    pub id: String,
    pub label: String,
    pub inspection_description: String,
    pub check: Option<EvidenceCheckDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceCheckDefinition {
    pub stat: String,
    pub difficulty_min_milli: u16,
    pub difficulty_max_milli: u16,
    pub success_description: String,
    pub reveals_clue: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessDemographicDefinition {
    pub id: String,
    pub label: String,
    pub match_rules: Vec<WitnessMatchRule>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessMatchRule {
    pub priority: i32,
    #[serde(default)]
    pub age_bands: Vec<String>,
    #[serde(default)]
    pub sexes: Vec<String>,
    #[serde(default)]
    pub professions: Vec<String>,
    #[serde(default)]
    pub local_roles: Vec<String>,
    #[serde(default)]
    pub fallback: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CircumstanceDefinition {
    pub id: String,
    pub statement: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SiteDefinition {
    pub id: String,
    pub label: String,
    pub terrain: String,
    pub habitat: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptionDefinition {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDefinition {
    pub id: String,
    pub label: String,
    pub routes: Vec<String>,
    pub objectives: Vec<String>,
    pub cause_finales: BTreeMap<String, Vec<String>>,
    pub consequence_profile: String,
    pub incident_interval_minutes: u64,
    pub maximum_incidents: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsequenceDefinition {
    pub id: String,
    pub family: String,
    pub causes: Vec<String>,
    pub symptom: String,
    pub buy_bps: i32,
    pub sell_penalty_bps: i32,
    pub encounter_frequency_bps: u16,
    pub encounter_archetype: Option<String>,
    pub disease_intensity: u16,
    pub public_summary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedRelation {
    pub id: String,
    pub candidates: Vec<WeightedCandidate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedCandidate {
    pub id: String,
    pub plausibility: u32,
    pub curation: u32,
    pub hard_zero_reason: Option<String>,
    pub required_bridge: Option<String>,
    #[serde(default)]
    pub factors: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeDefinition {
    pub id: String,
    pub explanation: String,
    pub lead_summary: String,
    pub event_suffix: String,
    pub evidence_id: String,
    pub action_ids: BTreeMap<String, String>,
}

#[derive(Debug)]
pub struct Catalog {
    pub documents: Vec<CatalogDocument>,
    monsters: BTreeMap<String, Monster>,
    evidence: BTreeMap<String, EvidenceDefinition>,
    sites: BTreeMap<String, SiteDefinition>,
    descriptions: BTreeMap<String, DescriptionDefinition>,
    templates: BTreeMap<String, TemplateDefinition>,
    relations: BTreeMap<String, WeightedRelation>,
}

impl Catalog {
    fn compile(documents: Vec<CatalogDocument>) -> Result<Self, String> {
        let mut monsters = BTreeMap::new();
        let mut evidence = BTreeMap::new();
        let mut sites = BTreeMap::new();
        let mut descriptions = BTreeMap::new();
        let mut templates = BTreeMap::new();
        let mut relations = BTreeMap::new();
        let mut consequence_ids = BTreeSet::new();
        let mut bridges = BTreeSet::new();
        let mut monster_ids = BTreeSet::new();
        for document in &documents {
            for bridge in &document.bridges {
                if !bridges.insert(bridge.id.clone()) {
                    return Err(format!("duplicate bridge {}", bridge.id));
                }
            }
            for monster in &document.monsters {
                validate_monster(monster)?;
                monster_ids.insert(monster.id.clone());
                if monsters
                    .insert(monster.id.clone(), monster.clone())
                    .is_some()
                {
                    return Err(format!("duplicate monster {}", monster.id));
                }
            }
            for item in &document.evidence {
                validate_evidence(item)?;
                if evidence.insert(item.id.clone(), item.clone()).is_some() {
                    return Err(format!("duplicate evidence {}", item.id));
                }
            }
            for item in &document.sites {
                if sites.insert(item.id.clone(), item.clone()).is_some() {
                    return Err(format!("duplicate site {}", item.id));
                }
            }
            for item in &document.descriptions {
                if descriptions.insert(item.id.clone(), item.clone()).is_some() {
                    return Err(format!("duplicate description {}", item.id));
                }
            }
            for item in &document.templates {
                if item.routes.is_empty() || item.objectives.is_empty() {
                    return Err(format!("template {} has no routes or objectives", item.id));
                }
                if item.cause_finales.is_empty()
                    || item.cause_finales.values().any(|finales| {
                        finales.is_empty() || finales.iter().any(|id| !item.objectives.contains(id))
                    })
                {
                    return Err(format!(
                        "template {} has invalid cause/finale coverage",
                        item.id
                    ));
                }
                if item.incident_interval_minutes == 0 || item.maximum_incidents == 0 {
                    return Err(format!(
                        "template {} has invalid incident scheduling",
                        item.id
                    ));
                }
                let (supported_routes, supported_objectives): (&[&str], &[&str]) = match item
                    .id
                    .as_str()
                {
                    "recurring_depredation" => (
                        &["physical_trail", "pattern_surveillance", "social_inquiry"],
                        &["defeat", "drive_off"],
                    ),
                    "disappearance_or_loss" => (
                        &["physical_trail", "social_inquiry"],
                        &["rescue", "retrieve_return", "expose"],
                    ),
                    _ => return Err(format!("template {} has no typed graph assembler", item.id)),
                };
                if item.routes.iter().map(String::as_str).collect::<Vec<_>>() != supported_routes
                    || item
                        .objectives
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        != supported_objectives
                {
                    return Err(format!(
                        "template {} requests an unsupported typed route/objective graph",
                        item.id
                    ));
                }
                if templates.insert(item.id.clone(), item.clone()).is_some() {
                    return Err(format!("duplicate template {}", item.id));
                }
            }
            for item in &document.consequences {
                if !consequence_ids.insert(item.id.clone())
                    || item.causes.is_empty()
                    || item.public_summary.trim().is_empty()
                    || item.encounter_frequency_bps > 10_000
                    || item.disease_intensity > 10_000
                    || item.buy_bps.unsigned_abs() > 10_000
                    || item.sell_penalty_bps.unsigned_abs() > 10_000
                {
                    return Err(format!("invalid or duplicate consequence {}", item.id));
                }
            }
            for item in &document.relations {
                if relations.insert(item.id.clone(), item.clone()).is_some() {
                    return Err(format!("duplicate relation {}", item.id));
                }
            }
        }
        for monster in monsters.values() {
            for other in &monster.investigation.mistaken_for {
                if !monster_ids.contains(other) {
                    return Err(format!(
                        "monster {} references missing mistaken_for monster {other}",
                        monster.id
                    ));
                }
            }
        }
        for relation in relations.values() {
            for candidate in &relation.candidates {
                if let Some(bridge) = &candidate.required_bridge {
                    if !bridges.contains(bridge) {
                        return Err(format!(
                            "relation {} references missing bridge {bridge}",
                            relation.id
                        ));
                    }
                }
            }
        }
        for monster in monsters.values() {
            for description in &monster.investigation.silhouettes {
                let relation_id = format!("description.{description}");
                let relation = relations.get(&relation_id).ok_or_else(|| {
                    format!(
                        "monster {} has unweighted description {description}",
                        monster.id
                    )
                })?;
                if !relation
                    .candidates
                    .iter()
                    .any(|candidate| candidate.id == monster.id && candidate.plausibility > 0)
                {
                    return Err(format!(
                        "monster {} description {description} has no positive authored likelihood",
                        monster.id
                    ));
                }
            }
        }
        for relation in relations
            .values()
            .filter(|item| item.id.starts_with("description."))
        {
            let description = relation.id.trim_start_matches("description.");
            if !descriptions.contains_key(description) {
                return Err(format!(
                    "relation {} references missing description",
                    relation.id
                ));
            }
            for candidate in &relation.candidates {
                if !monsters.contains_key(&candidate.id) {
                    return Err(format!(
                        "relation {} references missing monster {}",
                        relation.id, candidate.id
                    ));
                }
            }
        }
        Ok(Self {
            documents,
            monsters,
            evidence,
            sites,
            descriptions,
            templates,
            relations,
        })
    }

    pub fn monster(&self, id: &str) -> Option<&Monster> {
        self.monsters.get(id)
    }
    pub fn monsters(&self) -> impl Iterator<Item = &Monster> {
        self.monsters.values()
    }
    pub fn evidence(&self, id: &str) -> Option<&EvidenceDefinition> {
        self.evidence.get(id)
    }
    pub fn circumstance(&self, id: &str) -> Option<&CircumstanceDefinition> {
        self.documents
            .iter()
            .flat_map(|document| &document.circumstances)
            .find(|item| item.id == id)
    }
    pub fn witness_demographic_for(
        &self,
        age_band: &str,
        sex: &str,
        profession: &str,
        local_role: &str,
    ) -> Option<&WitnessDemographicDefinition> {
        self.documents
            .iter()
            .flat_map(|document| &document.witness_demographics)
            .flat_map(|demographic| {
                demographic
                    .match_rules
                    .iter()
                    .map(move |rule| (demographic, rule))
            })
            .filter(|(_, rule)| {
                rule.fallback
                    || (rule.age_bands.is_empty()
                        || rule.age_bands.iter().any(|value| value == age_band))
                        && (rule.sexes.is_empty() || rule.sexes.iter().any(|value| value == sex))
                        && (rule.professions.is_empty()
                            || rule.professions.iter().any(|value| {
                                crate::quest_catalog_validation::selector_matches_fact(
                                    value, profession,
                                )
                            }))
                        && (rule.local_roles.is_empty()
                            || rule.local_roles.iter().any(|value| {
                                crate::quest_catalog_validation::selector_matches_fact(
                                    value, local_role,
                                )
                            }))
            })
            .max_by_key(|(_, rule)| rule.priority)
            .map(|(demographic, _)| demographic)
    }
    pub fn site(&self, id: &str) -> Option<&SiteDefinition> {
        self.sites.get(id)
    }
    pub fn description(&self, id: &str) -> Option<&DescriptionDefinition> {
        self.descriptions.get(id)
    }
    pub fn template(&self, id: &str) -> Option<&TemplateDefinition> {
        self.templates.get(id)
    }
    pub fn relation(&self, id: &str) -> Option<&WeightedRelation> {
        self.relations.get(id)
    }
    pub fn bridge(&self, id: &str) -> Option<&BridgeDefinition> {
        self.documents
            .iter()
            .flat_map(|document| &document.bridges)
            .find(|bridge| bridge.id == id)
    }
    pub fn consequence(&self, family: &str, cause: &str) -> Option<&ConsequenceDefinition> {
        self.documents
            .iter()
            .flat_map(|document| &document.consequences)
            .find(|item| item.family == family && item.causes.iter().any(|id| id == cause))
            .or_else(|| {
                self.documents
                    .iter()
                    .flat_map(|document| &document.consequences)
                    .find(|item| item.family == family && item.causes.iter().any(|id| id == "*"))
            })
    }
}

fn validate_monster(monster: &Monster) -> Result<(), String> {
    if monster.combat.training_multiplier_milli == 0
        || monster.combat.encounter_scale_basis_points == 0
        || monster.combat.weight_kg <= 0.0
        || monster.combat.speed_m_per_minute == 0
    {
        return Err(format!("monster {} has invalid combat scalars", monster.id));
    }
    if monster.combat.resistance_joules > 20_000 || monster.combat.padding_joules > 20_000 {
        return Err(format!(
            "monster {} has implausible innate protection",
            monster.id
        ));
    }
    if monster.investigation.habitats.is_empty()
        || monster.investigation.silhouettes.is_empty()
        || monster.investigation.distinguishing_clues.is_empty()
    {
        return Err(format!(
            "monster {} has incomplete investigation data",
            monster.id
        ));
    }
    if !matches!(monster.combat.rig.as_str(), "humanoid" | "quadruped")
        || !matches!(
            monster.combat.attack.as_str(),
            "blade" | "blunt" | "knife" | "spear" | "bow" | "bite" | "claw" | "tusk"
        )
        || !matches!(
            monster.combat.protection.as_str(),
            "unarmored" | "hide" | "shielded" | "armored" | "bone" | "supernatural"
        )
        || !matches!(
            monster.combat.temperament.as_str(),
            "cowardly" | "cautious" | "disciplined" | "aggressive" | "relentless" | "elusive"
        )
    {
        return Err(format!(
            "monster {} names an unknown closed mechanic",
            monster.id
        ));
    }
    if monster.combat.ranged != (monster.combat.attack == "bow") {
        return Err(format!(
            "monster {} has inconsistent ranged attack mechanics",
            monster.id
        ));
    }
    Ok(())
}

fn validate_evidence(evidence: &EvidenceDefinition) -> Result<(), String> {
    if evidence.topics.is_empty() {
        return Err(format!("evidence {} has no inspection topics", evidence.id));
    }
    let mut topics = BTreeSet::new();
    for topic in &evidence.topics {
        if !topics.insert(&topic.id) {
            return Err(format!(
                "evidence {} has duplicate topic {}",
                evidence.id, topic.id
            ));
        }
        if let Some(check) = &topic.check {
            if check.difficulty_min_milli > check.difficulty_max_milli
                || check.difficulty_max_milli > 10_000
            {
                return Err(format!(
                    "evidence {} topic {} has invalid difficulty range",
                    evidence.id, topic.id
                ));
            }
            if !matches!(
                check.stat.as_str(),
                "eyesight" | "intelligence" | "instinct"
            ) {
                return Err(format!(
                    "evidence {} topic {} names unknown check stat {}",
                    evidence.id, topic.id, check.stat
                ));
            }
        }
    }
    Ok(())
}

pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let raw_documents: Vec<serde_json::Value> =
            serde_json::from_str(QUEST_CATALOG_JSON).expect("validated embedded quest catalog");
        let sources = raw_documents
            .iter()
            .enumerate()
            .map(|(index, _)| format!("embedded.catalog[{index}]"))
            .collect::<Vec<_>>();
        crate::quest_catalog_validation::validate_documents(&raw_documents, &sources)
            .expect("build-validated embedded quest catalog");
        let documents: Vec<CatalogDocument> = raw_documents
            .into_iter()
            .map(|value| serde_json::from_value(value).expect("strict validated catalog schema"))
            .collect();
        Catalog::compile(documents).expect("validated quest catalog references")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_compiles_and_is_sorted_by_lookup_key() {
        let catalog = catalog();
        assert!(catalog.monster("skeleton").is_some());
        assert!(catalog.evidence("footprints").is_some());
        assert_eq!(catalog.monsters().next().unwrap().id, "alp");
        assert_eq!(QUEST_CATALOG_DIGEST.len(), 64);
    }

    #[test]
    fn every_relation_keeps_plausibility_and_curation_separate() {
        for document in &catalog().documents {
            for relation in &document.relations {
                for candidate in &relation.candidates {
                    let zero = candidate.plausibility == 0 || candidate.curation == 0;
                    assert_eq!(zero, candidate.hard_zero_reason.is_some());
                }
            }
        }
    }

    fn raw_catalog() -> (Vec<serde_json::Value>, Vec<String>) {
        let documents: Vec<serde_json::Value> = serde_json::from_str(QUEST_CATALOG_JSON).unwrap();
        let files = (0..documents.len())
            .map(|index| format!("fixture[{index}]"))
            .collect();
        (documents, files)
    }

    #[test]
    fn shared_validator_rejects_unknown_fields_oversized_ids_and_layering() {
        let (documents, files) = raw_catalog();
        let mut unknown = documents.clone();
        unknown[0]["monsters"][0]["typo_field"] = serde_json::json!(true);
        assert!(
            crate::quest_catalog_validation::validate_documents(&unknown, &files)
                .unwrap_err()
                .contains("unknown field")
        );
        let mut oversized = documents.clone();
        oversized[0]["monsters"][0]["id"] = serde_json::json!("x".repeat(64));
        assert!(
            crate::quest_catalog_validation::validate_documents(&oversized, &files)
                .unwrap_err()
                .contains("bounded catalog ID")
        );
        let mut layered = documents;
        let monster = &mut layered[0]["monsters"][0];
        monster["combat"]["protection"] = serde_json::json!("armored");
        monster["combat"]["padding_joules"] = serde_json::json!(1);
        assert!(
            crate::quest_catalog_validation::validate_documents(&layered, &files)
                .unwrap_err()
                .contains("cannot compose")
        );
    }

    #[test]
    fn shared_validator_rejects_dangling_bridge_and_missing_demographic_fallback() {
        let (documents, files) = raw_catalog();
        let mut dangling = documents.clone();
        let relation_document = dangling
            .iter_mut()
            .find(|document| document["relations"].is_array())
            .unwrap();
        relation_document["relations"][0]["candidates"][0]["required_bridge"] =
            serde_json::json!("missing_bridge");
        assert!(
            crate::quest_catalog_validation::validate_documents(&dangling, &files)
                .unwrap_err()
                .contains("dangling bridge")
        );
        let mut no_fallback = documents;
        let demographic_document = no_fallback
            .iter_mut()
            .find(|document| document["witness_demographics"].is_array())
            .unwrap();
        for demographic in demographic_document["witness_demographics"]
            .as_array_mut()
            .unwrap()
        {
            for rule in demographic["match_rules"].as_array_mut().unwrap() {
                if rule["fallback"] == serde_json::json!(true) {
                    rule["fallback"] = serde_json::json!(false);
                    rule["age_bands"] = serde_json::json!(["adult"]);
                }
            }
        }
        assert!(
            crate::quest_catalog_validation::validate_documents(&no_fallback, &files)
                .unwrap_err()
                .contains("exactly one fallback")
        );
    }

    #[test]
    fn shared_validator_rejects_bad_dcs_graphs_relations_and_demographic_ties() {
        let (documents, files) = raw_catalog();
        let mut bad_dc = documents.clone();
        let investigation = bad_dc
            .iter_mut()
            .find(|document| document["evidence"].is_array())
            .unwrap();
        investigation["evidence"][0]["topics"][1]["check"]["difficulty_min_milli"] =
            serde_json::json!(9000);
        investigation["evidence"][0]["topics"][1]["check"]["difficulty_max_milli"] =
            serde_json::json!(1000);
        assert!(
            crate::quest_catalog_validation::validate_documents(&bad_dc, &files)
                .unwrap_err()
                .contains("invalid DC range")
        );

        let mut bad_graph = documents.clone();
        let generation = bad_graph
            .iter_mut()
            .find(|document| document["templates"].is_array())
            .unwrap();
        generation["templates"][0]["routes"] = serde_json::json!(["social_inquiry"]);
        assert!(
            crate::quest_catalog_validation::validate_documents(&bad_graph, &files)
                .unwrap_err()
                .contains("unsupported route/objective graph")
        );

        let mut dangling_candidate = documents.clone();
        let generation = dangling_candidate
            .iter_mut()
            .find(|document| document["relations"].is_array())
            .unwrap();
        let description = generation["relations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|relation| relation["id"] == "description.armed_people")
            .unwrap();
        description["candidates"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id":"missing_monster","plausibility":10,"curation":10,
                "hard_zero_reason":null,"required_bridge":null
            }));
        assert!(
            crate::quest_catalog_validation::validate_documents(&dangling_candidate, &files)
                .unwrap_err()
                .contains("dangling candidate")
        );

        let mut tied_demographics = documents;
        let investigation = tied_demographics
            .iter_mut()
            .find(|document| document["witness_demographics"].is_array())
            .unwrap();
        investigation["witness_demographics"][2]["match_rules"][0]["professions"] =
            serde_json::json!(["cleric"]);
        assert!(
            crate::quest_catalog_validation::validate_documents(&tied_demographics, &files)
                .unwrap_err()
                .contains("equal-priority demographic rules")
        );
    }

    #[test]
    fn shared_validator_rejects_runtime_type_overflow_optional_and_combat_mismatches() {
        let (documents, files) = raw_catalog();
        let validate = |documents: &Vec<serde_json::Value>| {
            crate::quest_catalog_validation::validate_documents(documents, &files).unwrap_err()
        };

        let mut priority = documents.clone();
        let investigation = priority
            .iter_mut()
            .find(|document| document["witness_demographics"].is_array())
            .unwrap();
        investigation["witness_demographics"][0]["match_rules"][0]["priority"] =
            serde_json::json!(i64::from(i32::MAX) + 1);
        assert!(validate(&priority).contains("priority"));

        let mut relation_weight = documents.clone();
        let generation = relation_weight
            .iter_mut()
            .find(|document| document["relations"].is_array())
            .unwrap();
        generation["relations"][0]["candidates"][0]["plausibility"] =
            serde_json::json!(u64::from(u32::MAX) + 1);
        assert!(validate(&relation_weight).contains("plausibility"));

        let mut typed_scalar = documents.clone();
        typed_scalar[0]["monsters"][0]["base_weight"] = serde_json::json!(u64::from(u16::MAX) + 1);
        assert!(validate(&typed_scalar).contains("base_weight"));

        for (field, invalid) in [
            ("loot_item_id", serde_json::json!(false)),
            ("loot_item_id", serde_json::json!(17)),
        ] {
            let mut optional = documents.clone();
            optional[0]["monsters"][0]["combat"][field] = invalid;
            assert!(validate(&optional).contains("string or null"));
        }

        let mut consequence_optional = documents.clone();
        let generation = consequence_optional
            .iter_mut()
            .find(|document| document["consequences"].is_array())
            .unwrap();
        generation["consequences"][0]["encounter_archetype"] = serde_json::json!(false);
        assert!(validate(&consequence_optional).contains("string or null"));

        let mut relation_optional = documents.clone();
        let generation = relation_optional
            .iter_mut()
            .find(|document| document["relations"].is_array())
            .unwrap();
        generation["relations"][0]["candidates"][0]["required_bridge"] = serde_json::json!(false);
        assert!(validate(&relation_optional).contains("string or null"));

        let mut hard_zero_optional = documents.clone();
        let generation = hard_zero_optional
            .iter_mut()
            .find(|document| document["relations"].is_array())
            .unwrap();
        generation["relations"][0]["candidates"][0]["hard_zero_reason"] = serde_json::json!(false);
        assert!(validate(&hard_zero_optional).contains("string or null"));

        let mut check_optional = documents.clone();
        let investigation = check_optional
            .iter_mut()
            .find(|document| document["evidence"].is_array())
            .unwrap();
        investigation["evidence"][0]["topics"][0]["check"] = serde_json::json!(false);
        assert!(validate(&check_optional).contains("expected object"));

        let mut factors = documents.clone();
        let generation = factors
            .iter_mut()
            .find(|document| document["relations"].is_array())
            .unwrap();
        generation["relations"][0]["candidates"][0]["factors"] = serde_json::json!(["valid", 7]);
        assert!(validate(&factors).contains("expected string"));

        let mut ranged = documents;
        ranged[0]["monsters"][0]["combat"]["ranged"] = serde_json::json!(true);
        assert!(validate(&ranged).contains("bow attack and ranged flag"));
    }

    #[test]
    fn demographic_selectors_use_exact_or_whole_token_matching() {
        assert!(crate::quest_catalog_validation::selector_matches_fact(
            "merchant", "merchant"
        ));
        assert!(!crate::quest_catalog_validation::selector_matches_fact(
            "mer", "merchant"
        ));
        assert!(!crate::quest_catalog_validation::selector_matches_fact(
            "chant", "merchant"
        ));
        assert!(crate::quest_catalog_validation::selector_matches_fact(
            "retainer",
            "lord's household retainer"
        ));

        let catalog = catalog();
        assert_eq!(
            catalog
                .witness_demographic_for("adult", "male", "merchant", "resident")
                .unwrap()
                .id,
            "merchant"
        );
        assert_eq!(
            catalog
                .witness_demographic_for("adult", "male", "mer", "resident")
                .unwrap()
                .id,
            "laborer"
        );
        assert_eq!(
            catalog
                .witness_demographic_for("adult", "male", "chant", "resident")
                .unwrap()
                .id,
            "laborer"
        );

        let (mut documents, files) = raw_catalog();
        let investigation = documents
            .iter_mut()
            .find(|document| document["witness_demographics"].is_array())
            .unwrap();
        investigation["witness_demographics"][2]["match_rules"][0]["professions"] =
            serde_json::json!(["mer"]);
        assert!(
            crate::quest_catalog_validation::validate_documents(&documents, &files)
                .unwrap_err()
                .contains("matches no authoritative NPC profession")
        );
    }

    #[test]
    fn yaml_only_open_ids_validate_without_rust_identity_edits() {
        let (mut documents, files) = raw_catalog();
        let monster_document = documents
            .iter_mut()
            .find(|document| document["monsters"].is_array())
            .unwrap();
        let mut monster = monster_document["monsters"][0].clone();
        monster["id"] = serde_json::json!("fixture_new_monster");
        monster["name"] = serde_json::json!("Fixture monster");
        monster["singular"] = serde_json::json!("Fixture monster");
        monster["plural"] = serde_json::json!("Fixture monsters");
        monster["investigation"]["mistaken_for"] = serde_json::json!(["bandit"]);
        monster_document["monsters"]
            .as_array_mut()
            .unwrap()
            .push(monster);
        let investigation_document = documents
            .iter_mut()
            .find(|document| document["evidence"].is_array())
            .unwrap();
        let mut evidence = investigation_document["evidence"][0].clone();
        evidence["id"] = serde_json::json!("fixture_new_evidence");
        investigation_document["evidence"]
            .as_array_mut()
            .unwrap()
            .push(evidence);
        let mut bridge = investigation_document["bridges"][0].clone();
        bridge["id"] = serde_json::json!("fixture_new_bridge");
        investigation_document["bridges"]
            .as_array_mut()
            .unwrap()
            .push(bridge);
        let mut demographic = investigation_document["witness_demographics"][0].clone();
        demographic["id"] = serde_json::json!("fixture_new_demographic");
        demographic["match_rules"][0]["age_bands"] = serde_json::json!(["elder"]);
        demographic["match_rules"][0]["priority"] = serde_json::json!(101);
        investigation_document["witness_demographics"]
            .as_array_mut()
            .unwrap()
            .push(demographic);
        let relation_document = documents
            .iter_mut()
            .find(|document| document["relations"].is_array())
            .unwrap();
        let description_relation = relation_document["relations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|relation| relation["id"] == "description.armed_people")
            .unwrap();
        description_relation["candidates"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id":"fixture_new_monster","plausibility":60,"curation":100,
                "hard_zero_reason":null,"required_bridge":"fixture_new_bridge"
            }));
        let evidence_relation = relation_document["relations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|relation| relation["id"] == "evidence.baseline")
            .unwrap();
        evidence_relation["candidates"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id":"fixture_new_evidence","plausibility":20,"curation":20,
                "hard_zero_reason":null,"required_bridge":null
            }));
        crate::quest_catalog_validation::validate_documents(&documents, &files).unwrap();
    }
}
