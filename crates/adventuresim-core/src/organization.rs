//! Build-time compiled organization content shared by strategic authority and UI.
//!
//! Stable strings cross persistence boundaries. Ranks, requirements, training,
//! recognition, and privileges are content rather than institution-shaped enums.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

include!(concat!(env!("OUT_DIR"), "/organization_catalog.rs"));

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationCatalog {
    pub organizations: Vec<OrganizationDefinition>,
    pub settlement_policies: Vec<SettlementPolicy>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub historical_fantasy_note: Option<String>,
    #[serde(default)]
    pub service_id: Option<String>,
    #[serde(default)]
    pub chapters: Vec<String>,
    pub recognition: Recognition,
    #[serde(default)]
    pub admission: Admission,
    #[serde(default)]
    pub dues: Option<Dues>,
    pub ranks: Vec<OrganizationRank>,
    pub activity: OrganizationActivity,
    #[serde(default)]
    pub privileges: Vec<Privilege>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recognition {
    Universal,
    Settlements { settlement_ids: Vec<String> },
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
    #[serde(default)]
    pub joining_fee: u32,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dues {
    pub amount: u32,
    pub interval_days: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationRank {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    pub practice_allowed: bool,
    pub practice_reward_interval_minutes: u32,
    #[serde(default)]
    pub privileges: Vec<Privilege>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Requirement {
    SkillRating {
        skill: String,
        minimum: f32,
        #[serde(default)]
        leaf: Option<String>,
    },
    ProfessedReligion {
        religion: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationActivity {
    #[serde(default)]
    pub training: Vec<TrainingEntry>,
    pub reward: ActivityReward,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TrainingEntry {
    pub weight: f32,
    #[serde(flatten)]
    pub target: TrainingTarget,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrainingTarget {
    FixedSkill { skill: String },
    Religion { religion: String },
    Bestiary { category: String },
    Terrain { terrain: String },
    EquippedWeaponSkills,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityReward {
    Gold,
    Virtue,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Privilege {
    BearArms,
    WearArmor,
    ForageHighGame,
    ForageLowGame,
    ForageFish,
    ForagePlants,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettlementPolicy {
    pub settlement_id: String,
    #[serde(default)]
    pub restrict_arms: bool,
    #[serde(default)]
    pub restrict_armor: bool,
}

impl Recognition {
    pub fn includes(&self, settlement_id: &str) -> bool {
        match self {
            Self::Universal => true,
            Self::Settlements { settlement_ids } => {
                settlement_ids.iter().any(|id| id == settlement_id)
            }
        }
    }
}

impl OrganizationDefinition {
    pub fn has_chapter(&self, settlement_id: &str) -> bool {
        self.chapters.iter().any(|id| id == settlement_id)
    }

    pub fn has_privilege(&self, privilege: Privilege) -> bool {
        self.privileges.contains(&privilege)
    }

    /// Organization privileges are inherited by every rank; rank privileges
    /// are additive.
    pub fn has_privilege_at_rank(&self, rank_id: &str, privilege: Privilege) -> bool {
        self.has_privilege(privilege)
            || self
                .rank(rank_id)
                .is_some_and(|rank| rank.privileges.contains(&privilege))
    }

    pub fn rank(&self, rank_id: &str) -> Option<&OrganizationRank> {
        self.ranks.iter().find(|rank| rank.id == rank_id)
    }

    pub fn next_rank(&self, rank_id: &str) -> Option<&OrganizationRank> {
        self.ranks
            .iter()
            .position(|rank| rank.id == rank_id)
            .and_then(|index| self.ranks.get(index + 1))
    }
}

pub fn catalog() -> &'static OrganizationCatalog {
    static CATALOG: OnceLock<OrganizationCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(ORGANIZATION_CATALOG_JSON)
            .expect("build-validated organization catalog must deserialize")
    })
}

pub fn organization(id: &str) -> Option<&'static OrganizationDefinition> {
    catalog().organizations.iter().find(|entry| entry.id == id)
}

pub fn organizations_for_chapter(
    settlement_id: &str,
) -> impl Iterator<Item = &'static OrganizationDefinition> {
    catalog()
        .organizations
        .iter()
        .filter(move |organization| organization.has_chapter(settlement_id))
}

pub fn settlement_policy(settlement_id: &str) -> Option<&'static SettlementPolicy> {
    catalog()
        .settlement_policies
        .iter()
        .find(|policy| policy.settlement_id == settlement_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_rank_names_and_mixed_requirements_round_trip() {
        let definition = organization("test_catholic_polearm_cooks").unwrap();
        let rank = definition.rank("keeper_of_the_long_spoon").unwrap();
        assert_eq!(rank.name, "Keeper of the Long Spoon");
        assert!(matches!(
            &rank.requirements[0],
            Requirement::SkillRating { skill, minimum, .. }
                if skill == "cooking" && *minimum == 4.0
        ));
        assert!(matches!(
            &rank.requirements[1],
            Requirement::SkillRating { skill, minimum, .. }
                if skill == "polearm" && *minimum == 4.0
        ));
    }

    #[test]
    fn one_rank_organization_can_practice_from_yaml() {
        let definition = organization("lutheran_learned_visitation").unwrap();
        let rank = definition.rank("hearer").unwrap();
        assert!(rank.practice_allowed);
        assert_eq!(rank.practice_reward_interval_minutes, 480);
    }

    #[test]
    fn ranger_common_licenses_are_inherited_and_high_game_is_master_only() {
        for id in [
            "wardens_harz",
            "keepers_solling",
            "company_green_staff_thuringia",
        ] {
            let definition = organization(id).unwrap();
            let first = &definition.ranks[0];
            assert!(definition.has_privilege_at_rank(&first.id, Privilege::ForageLowGame));
            assert!(definition.has_privilege_at_rank(&first.id, Privilege::ForageFish));
            assert!(definition.has_privilege_at_rank(&first.id, Privilege::ForagePlants));
            assert!(!definition.has_privilege_at_rank(&first.id, Privilege::ForageHighGame));
            assert!(definition.has_privilege_at_rank("master", Privilege::ForageHighGame));
        }
    }
}
