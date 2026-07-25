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
pub struct EvidenceDefinition {
    pub id: String,
    pub portrait_label: String,
    pub portrait_icon: String,
    pub base_description: String,
    pub topics: Vec<EvidenceTopicDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EvidenceTopicDefinition {
    pub id: String,
    pub label: String,
    pub inspection_description: String,
    pub check: Option<EvidenceCheckDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EvidenceCheckDefinition {
    pub stat: String,
    pub difficulty_min_milli: u16,
    pub difficulty_max_milli: u16,
    pub success_description: String,
    pub reveals_clue: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WitnessDemographicDefinition {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CircumstanceDefinition {
    pub id: String,
    pub statement: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SiteDefinition {
    pub id: String,
    pub label: String,
    pub terrain: String,
    pub habitat: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DescriptionDefinition {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
pub struct WeightedRelation {
    pub id: String,
    pub candidates: Vec<WeightedCandidate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
pub struct BridgeDefinition {
    pub id: String,
    pub explanation: String,
    pub lead_summary: String,
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
        let documents: Vec<CatalogDocument> =
            serde_json::from_str(QUEST_CATALOG_JSON).expect("validated embedded quest catalog");
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
}
