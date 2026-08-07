//! Build-time compiled organization content shared by strategic authority and UI.
//!
//! Stable strings cross persistence boundaries. Roles, requirements, training,
//! recognition, and privileges are content rather than institution-shaped enums.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

include!(concat!(env!("OUT_DIR"), "/organization_catalog.rs"));

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationCatalog {
    pub organizations: Vec<OrganizationDefinition>,
    pub promotion_transitions: Vec<OrganizationPromotionTransition>,
    pub settlement_policies: Vec<SettlementPolicy>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationPromotionTransition {
    pub organization_id: String,
    pub from_role_id: String,
    pub to_role_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecondPersonRegister {
    pub subject: &'static str,
    pub object: &'static str,
    pub possessive: &'static str,
    pub possessive_pronoun: &'static str,
    pub reflexive: &'static str,
    pub be: &'static str,
    pub have: &'static str,
    pub do_word: &'static str,
    pub will: &'static str,
    pub may: &'static str,
    pub should: &'static str,
}

/// Early Modern singular familiar address is downward or intimate. Plural
/// address is always `you`, regardless of either participant's social roles.
pub const fn second_person_register(
    singular: bool,
    speaker_outranks_addressee: bool,
    intimate: bool,
) -> SecondPersonRegister {
    if singular && (speaker_outranks_addressee || intimate) {
        SecondPersonRegister {
            subject: "thou",
            object: "thee",
            possessive: "thy",
            possessive_pronoun: "thine",
            reflexive: "thyself",
            be: "art",
            have: "hast",
            do_word: "dost",
            will: "wilt",
            may: "mayst",
            should: "shouldst",
        }
    } else {
        SecondPersonRegister {
            subject: "you",
            object: "you",
            possessive: "your",
            possessive_pronoun: "yours",
            reflexive: "yourself",
            be: "are",
            have: "have",
            do_word: "do",
            will: "will",
            may: "may",
            should: "should",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: OrganizationKind,
    #[serde(default)]
    pub historical_fantasy_note: Option<String>,
    #[serde(default)]
    pub service_id: Option<String>,
    /// Authored authority to refer dues-current members to publicly notorious
    /// hostile cases. Never inferred from names, roles, or skills.
    pub public_threat_referrals: bool,
    /// Authored authority to issue romantic errantry. This capability is
    /// intentionally narrower than public threat referral.
    #[serde(default)]
    pub errantry_issuance: bool,
    #[serde(default)]
    pub starting_role: Option<OrganizationStartingRole>,
    /// The sole profession-bearing membership tiers for this organization.
    /// A character has exactly one of these roles in each organization instance.
    pub roles: Vec<OrganizationRoleDefinition>,
    /// Roles available through ordinary admission. Internal assignment may
    /// select other disconnected roles explicitly (family, civic status, &c.).
    #[serde(default)]
    pub entry_role_ids: Vec<String>,
    #[serde(default)]
    pub chapters: Vec<OrganizationChapter>,
    pub recognition: Recognition,
    #[serde(default)]
    pub admission: Admission,
    #[serde(default)]
    pub dues: Option<Dues>,
    pub activity: OrganizationActivity,
    #[serde(default)]
    pub privileges: Vec<Privilege>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationKind {
    ProfessionalAssociation,
    ReligiousOrganization,
    NobleHouse,
    Lordship,
    CivicCommunity,
    Family,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OrganizationRoleDefinition {
    pub id: String,
    pub name: String,
    /// The social or occupational identity supplied by this role. Unlike the
    /// former social scalar, this is meaningful only with its organization
    /// instance (a citizen of Lubeck, a noble of a particular house, &c.).
    pub profession: String,
    /// Higher values win when dialogue chooses a public form of address.
    #[serde(default)]
    pub address_priority: i16,
    /// Independent ordering used only to decide whether a speaker addresses a
    /// social inferior with singular familiar pronouns.
    #[serde(default)]
    pub social_precedence: i16,
    /// Spoken title, without a personal name (for example "Father" or
    /// "my lord"). An empty title leaves the actor's ordinary profession.
    #[serde(default)]
    pub address_title: String,
    /// Private or concealed roles never become dialogue identity merely
    /// because authoritative persistence knows of them.
    #[serde(default)]
    pub publicly_recognizable: bool,
    /// An authored upbringing or institutional entitlement. This replaces the
    /// old hard-coded assumption that every noble is literate.
    #[serde(default)]
    pub creation_literacy: Option<adventuresim_world_schema::WrittenLanguage>,
    pub description: String,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    pub practice_allowed: bool,
    pub practice_reward_interval_minutes: u32,
    #[serde(default)]
    pub privileges: Vec<Privilege>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitialSocialRole {
    pub definition_id: &'static str,
    pub role_id: &'static str,
    pub settlement_scoped: bool,
}

/// Order-independent first-pass social assignment. Callers supply an explicit
/// actor-domain key so durable Characters and settlement NPCs cannot share an
/// entropy stream. The persistence-contract stable hash is used deliberately;
/// reducers must not consume RNG for this assignment.
pub fn initial_social_role(
    actor_domain_key: &str,
    settlement_id: &str,
    urban: bool,
) -> InitialSocialRole {
    let draw = crate::settlement_population::stable_hash(&format!(
        "social-role:v3:{}:{settlement_id}:{actor_domain_key}",
        settlement_id.len()
    )) % 4;
    match draw {
        0 => InitialSocialRole {
            definition_id: "local_lordship",
            role_id: "serf",
            settlement_scoped: true,
        },
        1 => InitialSocialRole {
            definition_id: "settlement_civic_community",
            role_id: "free_resident",
            settlement_scoped: true,
        },
        2 if urban => InitialSocialRole {
            definition_id: "settlement_civic_community",
            role_id: "citizen",
            settlement_scoped: true,
        },
        2 => InitialSocialRole {
            definition_id: "settlement_civic_community",
            role_id: "free_resident",
            settlement_scoped: true,
        },
        _ => InitialSocialRole {
            definition_id: "local_noble_house",
            role_id: "house_member",
            settlement_scoped: true,
        },
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationChapter {
    pub settlement_id: String,
    /// Stable settlement-scoped chapter identity and standalone navigation ID.
    /// A service-linked chapter may derive a different physical NPC location.
    pub location_id: String,
    pub building_name: String,
    pub building_kind: ChapterBuildingKind,
    pub representative_title: String,
    pub representative_profession: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChapterBuildingKind {
    Guildhall,
    Workshop,
    College,
    Confraternity,
    Commandery,
    Lodge,
}

/// Explicit character-creation metadata. This is deliberately authored rather
/// than inferred from services, names, requirements, or training skills.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationStartingRole {
    pub profession: StartingProfession,
    pub adult_role_id: String,
    pub old_role_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartingProfession {
    Merchant,
    Weaponsmith,
    Armourer,
    Tailor,
    Herbalist,
    Cook,
    LearnedReligiousPractitioner,
    WitchHunter,
    Knight,
    Forester,
}

impl StartingProfession {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Merchant => "merchant",
            Self::Weaponsmith => "weaponsmith",
            Self::Armourer => "armourer",
            Self::Tailor => "tailor",
            Self::Herbalist => "herbalist",
            Self::Cook => "cook",
            Self::LearnedReligiousPractitioner => "learned_religious_practitioner",
            Self::WitchHunter => "witch_hunter",
            Self::Knight => "knight",
            Self::Forester => "forester",
        }
    }
    pub const ALL: [Self; 10] = [
        Self::Merchant,
        Self::Weaponsmith,
        Self::Armourer,
        Self::Tailor,
        Self::Herbalist,
        Self::Cook,
        Self::LearnedReligiousPractitioner,
        Self::WitchHunter,
        Self::Knight,
        Self::Forester,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Merchant => "Merchant",
            Self::Weaponsmith => "Weaponsmith",
            Self::Armourer => "Armourer",
            Self::Tailor => "Tailor",
            Self::Herbalist => "Herbalist",
            Self::Cook => "Cook",
            Self::LearnedReligiousPractitioner => "Learned religious practitioner",
            Self::WitchHunter => "Witch hunter",
            Self::Knight => "Knight",
            Self::Forester => "Forester",
        }
    }
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
    FixedSkill {
        skill: String,
    },
    Religion {
        religion: String,
    },
    Bestiary {
        category: String,
    },
    Terrain {
        terrain: String,
    },
    Written {
        language: adventuresim_world_schema::WrittenLanguage,
    },
    EquippedWeaponSkills,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityReward {
    Gold,
    Fame,
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
        self.chapter(settlement_id).is_some()
    }

    pub fn chapter(&self, settlement_id: &str) -> Option<&OrganizationChapter> {
        self.chapters
            .iter()
            .find(|chapter| chapter.settlement_id == settlement_id)
    }

    pub fn chapter_at_location(
        &self,
        settlement_id: &str,
        location_id: &str,
    ) -> Option<&OrganizationChapter> {
        self.chapters.iter().find(|chapter| {
            chapter.settlement_id == settlement_id && chapter.location_id == location_id
        })
    }

    pub fn has_privilege(&self, privilege: Privilege) -> bool {
        self.privileges.contains(&privilege)
    }

    /// Organization privileges are inherited by every role; role privileges
    /// are additive.
    pub fn has_privilege_at_role(&self, role_id: &str, privilege: Privilege) -> bool {
        self.has_privilege(privilege)
            || self
                .role(role_id)
                .is_some_and(|role| role.privileges.contains(&privilege))
    }

    pub fn role(&self, role_id: &str) -> Option<&OrganizationRoleDefinition> {
        self.roles.iter().find(|role| role.id == role_id)
    }

    pub fn promotion_targets(
        &self,
        role_id: &str,
    ) -> impl Iterator<Item = &OrganizationRoleDefinition> {
        catalog()
            .promotion_transitions
            .iter()
            .filter(move |edge| edge.organization_id == self.id && edge.from_role_id == role_id)
            .filter_map(|edge| self.role(&edge.to_role_id))
    }

    pub fn can_transition(&self, from_role_id: &str, to_role_id: &str) -> bool {
        catalog().promotion_transitions.iter().any(|edge| {
            edge.organization_id == self.id
                && edge.from_role_id == from_role_id
                && edge.to_role_id == to_role_id
        })
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

pub fn organization_chapter_at(
    settlement_id: &str,
    location_id: &str,
) -> Option<(
    &'static OrganizationDefinition,
    &'static OrganizationChapter,
)> {
    catalog().organizations.iter().find_map(|organization| {
        organization
            .chapter_at_location(settlement_id, location_id)
            .map(|chapter| (organization, chapter))
    })
}

/// Ordinary settlement venue that can physically host representatives for a
/// service-linked organization. The authored chapter location remains the
/// chapter's stable institutional identity.
pub fn service_npc_location_id(service_id: &str) -> Option<&'static str> {
    match service_id {
        "merchants" => Some("market"),
        "weapons" => Some("forge"),
        "armor" => Some("armoury"),
        "clothing" => Some("tailor"),
        "books" => Some("bookstore"),
        "herbalist" => Some("herbalist"),
        "inn" => Some("inn"),
        "religion" => Some("church"),
        _ => None,
    }
}

pub fn service_npc_location_available(
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    service_id: &str,
) -> bool {
    use crate::settlement_economy::{
        SettlementActionService, Storefront, action_service_available, storefront_available,
    };
    match service_id {
        "merchants" => storefront_available(profile, Storefront::General),
        "weapons" => storefront_available(profile, Storefront::Weapons),
        "armor" => storefront_available(profile, Storefront::Armor),
        "clothing" => storefront_available(profile, Storefront::Clothing),
        "books" => storefront_available(profile, Storefront::Books),
        "herbalist" => storefront_available(profile, Storefront::Herbalist),
        "inn" => action_service_available(profile, SettlementActionService::Inn),
        "religion" => action_service_available(profile, SettlementActionService::Temple),
        _ => false,
    }
}

pub fn chapter_effective_location_id<'a>(
    organization: &'a OrganizationDefinition,
    chapter: &'a OrganizationChapter,
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
) -> &'a str {
    if let Some(service_id) = organization.service_id.as_deref()
        && service_npc_location_available(profile, service_id)
        && let Some(location_id) = service_npc_location_id(service_id)
    {
        return location_id;
    }
    &chapter.location_id
}

pub fn chapter_has_standalone_building(
    organization: &OrganizationDefinition,
    chapter: &OrganizationChapter,
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
) -> bool {
    chapter_effective_location_id(organization, chapter, profile) == chapter.location_id.as_str()
}

/// Stable representative identity is deliberately independent of provider
/// ordinals and physical venue so co-location cannot collide with service NPCs.
pub fn organization_representative_id(settlement_id: &str, organization_id: &str) -> u64 {
    crate::settlement_population::stable_hash(&format!(
        "resident:organization-representative:{settlement_id}:{organization_id}"
    )) | (1u64 << 63)
}

pub fn exact_representative_fields_match(
    resident_character_id: u64,
    expected_id: u64,
    home_settlement_id: &str,
    settlement_id: &str,
    organization_id: &str,
    expected_organization_id: &str,
    conversation_id: &str,
) -> bool {
    resident_character_id == expected_id
        && home_settlement_id == settlement_id
        && organization_id == expected_organization_id
        && conversation_id == "organization-representative"
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
    fn second_person_register_is_asymmetric_and_plural_safe() {
        let downward = second_person_register(true, true, false);
        assert_eq!(
            (
                downward.subject,
                downward.object,
                downward.be,
                downward.have
            ),
            ("thou", "thee", "art", "hast")
        );
        assert_eq!((downward.may, downward.should), ("mayst", "shouldst"));
        let upward = second_person_register(true, false, false);
        assert_eq!(
            (upward.subject, upward.object, upward.be, upward.have),
            ("you", "you", "are", "have")
        );
        let intimate = second_person_register(true, false, true);
        assert_eq!(intimate.reflexive, "thyself");
        let plural = second_person_register(false, true, true);
        assert_eq!(
            (plural.subject, plural.possessive, plural.will),
            ("you", "your", "will")
        );
        assert_eq!((plural.may, plural.should), ("may", "should"));
    }

    #[test]
    fn explicit_transitions_allow_disconnected_and_branching_roles() {
        let family = organization("common_family").unwrap();
        assert!(family.promotion_targets("family_member").next().is_none());
        let cooks = organization("test_catholic_polearm_cooks").unwrap();
        assert_eq!(cooks.promotion_targets("spoon_bearer").count(), 2);
        assert!(cooks.can_transition("spoon_bearer", "keeper_of_the_long_spoon"));
        assert!(cooks.can_transition("spoon_bearer", "keeper_of_the_silver_spoon"));
        assert!(!cooks.can_transition("keeper_of_the_long_spoon", "spoon_bearer"));
    }

    #[test]
    fn arbitrary_role_names_and_mixed_requirements_round_trip() {
        let definition = organization("test_catholic_polearm_cooks").unwrap();
        let role = definition.role("keeper_of_the_long_spoon").unwrap();
        assert_eq!(role.name, "Keeper of the Long Spoon");
        assert!(matches!(
            &role.requirements[0],
            Requirement::SkillRating { skill, minimum, .. }
                if skill == "cooking" && *minimum == 4.0
        ));
        assert!(matches!(
            &role.requirements[1],
            Requirement::SkillRating { skill, minimum, .. }
                if skill == "polearm" && *minimum == 4.0
        ));
    }

    #[test]
    fn one_role_organization_can_practice_from_yaml() {
        let definition = organization("lutheran_learned_visitation").unwrap();
        let role = definition.role("hearer").unwrap();
        assert!(role.practice_allowed);
        assert_eq!(role.practice_reward_interval_minutes, 480);
    }

    #[test]
    fn medical_crafts_have_distinct_organizations_and_curricula() {
        let herbalists = organization("herbalists_fellowship").unwrap();
        let physicians = organization("physicians_college").unwrap();
        let surgeons = organization("surgeons_guild").unwrap();

        assert_eq!(herbalists.service_id.as_deref(), Some("herbalist"));
        assert_eq!(physicians.service_id.as_deref(), Some("physician"));
        assert_eq!(surgeons.service_id.as_deref(), Some("surgeon"));
        let herbalist_start = herbalists.starting_role.as_ref().unwrap();
        assert_eq!(herbalist_start.adult_role_id, "herbalist");
        assert_eq!(herbalist_start.old_role_id, "elder_herbalist");
        assert_eq!(herbalists.name, "Fellowship of Herbalists");
        assert_eq!(herbalists.admission.joining_fee, 0);
        assert_eq!(
            herbalists
                .roles
                .iter()
                .map(|role| role.id.as_str())
                .collect::<Vec<_>>(),
            ["learner", "herbalist", "elder_herbalist"]
        );
        assert!(physicians.starting_role.is_none());
        assert!(surgeons.starting_role.is_none());

        let fixed_curriculum = |definition: &OrganizationDefinition| -> Vec<(String, f32)> {
            definition
                .activity
                .training
                .iter()
                .map(|entry| match &entry.target {
                    TrainingTarget::FixedSkill { skill } => (skill.clone(), entry.weight),
                    other => panic!("unexpected medical training target: {other:?}"),
                })
                .collect()
        };
        assert_eq!(
            fixed_curriculum(herbalists),
            vec![("herbalism".into(), 1.0)]
        );
        assert_eq!(
            fixed_curriculum(physicians),
            vec![("physiology".into(), 1.0)]
        );
        assert_eq!(fixed_curriculum(surgeons), vec![("surgery".into(), 1.0)]);
        for (definition, role_id, expected_skill) in [
            (physicians, "physician", "physiology"),
            (physicians, "master_physician", "physiology"),
            (surgeons, "surgeon", "surgery"),
            (surgeons, "master_surgeon", "surgery"),
        ] {
            let requirements = &definition.role(role_id).unwrap().requirements;
            assert_eq!(requirements.len(), 1);
            assert!(matches!(
                &requirements[0],
                Requirement::SkillRating { skill, .. } if skill == expected_skill
            ));
        }

        for settlement in ["viabundus-0", "viabundus-2337", "viabundus-1826"] {
            let locations = [
                herbalists.chapter(settlement).unwrap().location_id.as_str(),
                physicians.chapter(settlement).unwrap().location_id.as_str(),
                surgeons.chapter(settlement).unwrap().location_id.as_str(),
            ];
            assert_eq!(
                locations
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                3
            );
        }
    }

    #[test]
    fn ranger_common_licenses_are_inherited_and_high_game_is_master_only() {
        let definition = organization("lodge_hart_king").unwrap();
        let first = &definition.roles[0];
        assert!(definition.has_privilege_at_role(&first.id, Privilege::ForageLowGame));
        assert!(definition.has_privilege_at_role(&first.id, Privilege::ForageFish));
        assert!(definition.has_privilege_at_role(&first.id, Privilege::ForagePlants));
        assert!(!definition.has_privilege_at_role(&first.id, Privilege::ForageHighGame));
        assert!(definition.has_privilege_at_role("master", Privilege::ForageHighGame));
    }

    #[test]
    fn adventurer_professions_each_have_one_neutral_starting_organization() {
        for (profession, id, name, chapter_count) in [
            (
                StartingProfession::WitchHunter,
                "hunt_pale_lantern",
                "The Hunt of the Pale Lantern",
                2,
            ),
            (
                StartingProfession::Knight,
                "order_saint_george",
                "The Order of St. George",
                2,
            ),
            (
                StartingProfession::Forester,
                "lodge_hart_king",
                "The Lodge of the Hart King",
                3,
            ),
        ] {
            let eligible = catalog()
                .organizations
                .iter()
                .filter(|definition| {
                    definition
                        .starting_role
                        .as_ref()
                        .is_some_and(|role| role.profession == profession)
                })
                .collect::<Vec<_>>();
            assert_eq!(eligible.len(), 1, "{profession:?}");
            let definition = eligible[0];
            assert_eq!(definition.id, id);
            assert_eq!(definition.name, name);
            assert_eq!(definition.chapters.len(), chapter_count);
            assert!(definition.admission.requirements.iter().all(|requirement| {
                !matches!(requirement, Requirement::ProfessedReligion { .. })
            }));
            assert!(definition.roles.iter().all(|role| {
                role.requirements.iter().all(|requirement| {
                    !matches!(requirement, Requirement::ProfessedReligion { .. })
                })
            }));
            assert!(definition.public_threat_referrals);
        }

        let hunt = organization("hunt_pale_lantern").unwrap();
        assert!(
            hunt.activity
                .training
                .iter()
                .all(|entry| { !matches!(&entry.target, TrainingTarget::Religion { .. }) })
        );
        assert_eq!(
            hunt.activity.training,
            vec![
                TrainingEntry {
                    weight: 0.5,
                    target: TrainingTarget::Bestiary {
                        category: "spirit".into(),
                    },
                },
                TrainingEntry {
                    weight: 0.5,
                    target: TrainingTarget::EquippedWeaponSkills,
                },
            ]
        );
    }

    #[test]
    fn starting_profession_ids_are_authoring_stable_snake_case() {
        assert_eq!(
            StartingProfession::LearnedReligiousPractitioner.id(),
            "learned_religious_practitioner"
        );
        assert_eq!(StartingProfession::WitchHunter.id(), "witch_hunter");
    }

    #[test]
    fn role_identity_and_hierarchy_are_authored_on_real_roles() {
        let prince = organization("house_habsburg")
            .unwrap()
            .role("prince")
            .unwrap();
        assert_eq!(prince.profession, "prince");
        assert!(prince.social_precedence > 300);
        let priest = organization("lutheran_learned_visitation")
            .unwrap()
            .roles
            .iter()
            .find(|role| role.profession == "learned_religious_practitioner")
            .unwrap();
        assert_eq!(priest.profession, "learned_religious_practitioner");
        assert!(priest.address_priority > prince.address_priority);
        assert!(priest.social_precedence > prince.social_precedence);
    }

    #[test]
    fn every_denomination_has_an_honest_clerical_profession_role() {
        for id in [
            "roman_catholic_learned_chapter",
            "lutheran_learned_visitation",
            "reformed_learned_chapter",
            "anglican_learned_fellowship",
            "orthodox_learned_brotherhood",
            "islamic_learned_fellowship",
            "jewish_learned_fellowship",
        ] {
            let definition = organization(id).unwrap();
            assert!(
                definition
                    .roles
                    .iter()
                    .any(|role| role.profession == "learned_religious_practitioner"),
                "{id}"
            );
        }
        assert!(
            organization("reformed_learned_chapter")
                .unwrap()
                .roles
                .iter()
                .any(|role| role.profession == "learned_religious_practitioner")
        );
    }

    #[test]
    fn civic_noble_and_serf_positions_are_organization_professions() {
        let civic = organization("settlement_civic_community").unwrap();
        for (role_id, profession) in [("free_resident", "free_resident"), ("citizen", "citizen")] {
            assert_eq!(civic.role(role_id).unwrap().profession, profession);
        }
        assert_eq!(
            organization("local_noble_house")
                .unwrap()
                .role("house_member")
                .unwrap()
                .profession,
            "noble"
        );
        assert_eq!(
            organization("local_noble_house")
                .unwrap()
                .role("house_member")
                .unwrap()
                .creation_literacy,
            Some(adventuresim_world_schema::WrittenLanguage::German)
        );
        assert_eq!(civic.role("citizen").unwrap().creation_literacy, None);
        assert_eq!(
            organization("local_lordship")
                .unwrap()
                .role("serf")
                .unwrap()
                .profession,
            "serf"
        );
    }

    #[test]
    fn address_priority_makes_clergy_overwrite_noble_and_noble_overwrite_citizen() {
        let citizen = organization("settlement_civic_community")
            .unwrap()
            .role("citizen")
            .unwrap();
        let noble = organization("local_noble_house")
            .unwrap()
            .role("house_member")
            .unwrap();
        let clergy = organization("roman_catholic_learned_chapter")
            .unwrap()
            .roles
            .iter()
            .find(|role| role.profession == "learned_religious_practitioner")
            .unwrap();
        assert!(clergy.address_priority > noble.address_priority);
        assert!(noble.address_priority > citizen.address_priority);
        assert!(clergy.social_precedence > noble.social_precedence);
        assert!(noble.social_precedence > citizen.social_precedence);
        assert_eq!(clergy.profession, "learned_religious_practitioner");
        assert_eq!(noble.profession, "noble");
        assert_eq!(citizen.profession, "citizen");
    }

    #[test]
    fn deterministic_social_assignment_covers_authored_organization_roles() {
        let mut found = std::collections::BTreeMap::new();
        for id in 0..10_000 {
            let role = initial_social_role(&format!("character:{id}"), "settlement-a", true);
            found.entry(role.role_id).or_insert((id, role));
            if found.len() == 4 {
                break;
            }
        }
        assert_eq!(
            found.keys().copied().collect::<Vec<_>>(),
            ["citizen", "free_resident", "house_member", "serf"]
        );
        assert_eq!(found["house_member"].1.definition_id, "local_noble_house");
        assert_eq!(found["serf"].1.definition_id, "local_lordship");
        assert!(found["house_member"].1.settlement_scoped);
        assert!(found["serf"].1.settlement_scoped);
        for (id, expected) in found.values() {
            assert_eq!(
                initial_social_role(&format!("character:{id}"), "settlement-a", true),
                *expected
            );
            assert_ne!(
                crate::settlement_population::stable_hash(&format!(
                    "social-role:v3:12:settlement-a:character:{id}"
                )),
                crate::settlement_population::stable_hash(&format!(
                    "social-role:v3:12:settlement-a:settlement-npc:{id}"
                ))
            );
        }
        let urban_burgher = (0..10_000)
            .find(|id| {
                initial_social_role(&format!("character:{id}"), "settlement-a", true).role_id
                    == "citizen"
            })
            .unwrap();
        let rural =
            initial_social_role(&format!("character:{urban_burgher}"), "settlement-a", false);
        assert_eq!(rural.role_id, "free_resident");
        assert_ne!(
            crate::settlement_population::stable_hash("social-role:v3:12:settlement-a:character:7"),
            crate::settlement_population::stable_hash("social-role:v3:12:settlement-b:character:7")
        );
    }

    #[test]
    fn social_persistence_hooks_cover_residents_as_full_characters() {
        let social =
            include_str!("../../adventuresim-stdb-module/src/social_roles.rs").replace('\r', "");
        let character =
            include_str!("../../adventuresim-stdb-module/src/character.rs").replace('\r', "");
        let population =
            include_str!("../../adventuresim-stdb-module/src/settlement_population.rs")
                .replace('\r', "");
        let dialogue =
            include_str!("../../adventuresim-stdb-module/src/strategic/dialogue_provenance.rs")
                .replace('\r', "");
        let world_import =
            include_str!("../../adventuresim-stdb-module/src/strategic/world_import.rs")
                .replace('\r', "");
        assert!(!social.contains("character_estate_basis"));
        assert!(social.contains("format!(\"character:{character_id}:{instance_id}\")"));
        assert!(social.contains("already has a different role in this organization instance"));
        assert!(social.contains("row.organization_instance_id == instance_id"));
        assert!(!social.contains("resident_character_id"));
        assert!(!social.contains("ctx.random"));
        assert!(social.contains("\"lutheran\" => \"lutheran_learned_visitation\""));
        assert!(social.contains("\"reformed\" => \"reformed_learned_chapter\""));
        assert!(character.contains("ensure_character_social_roles("));
        assert!(character.contains("ensure_character_professional_role("));
        assert!(character.contains("delete_character_social_roles(ctx, character.id)"));
        assert!(population.contains("pub struct SettlementResidentProfile"));
        assert!(population.contains("ensure_character_social_roles("));
        assert!(population.contains("ensure_settlement_social_organizations(ctx, settlement_id)"));
        assert!(world_import.contains("delete_character_for_world_import(ctx, character)"));
        assert!(world_import.contains("delete_unreferenced_settlement_social_organizations("));
        let mission =
            include_str!("../../adventuresim-stdb-module/src/strategic/mission_bootstrap.rs")
                .replace('\r', "");
        assert!(!mission.contains("copy_settlement_resident_social_roles_to_character("));
        assert!(mission.contains("let leader_id = npc.character_id"));
        assert!(mission.contains("settlement_resident_id: npc.character_id"));
        assert!(dialogue.matches("FactKey::ParticipantRole").count() >= 2);
        assert!(dialogue.contains("character_roles(ctx, id)?"));
        assert!(dialogue.contains("character_roles(ctx, npc.character_id)?"));
    }

    #[test]
    fn chapter_locations_are_stable_distinct_and_reverse_resolvable() {
        let mut seen = std::collections::BTreeSet::new();
        for organization in &catalog().organizations {
            for chapter in &organization.chapters {
                assert!(seen.insert((chapter.settlement_id.clone(), chapter.location_id.clone())));
                let (found, found_chapter) =
                    organization_chapter_at(&chapter.settlement_id, &chapter.location_id).unwrap();
                assert_eq!(found.id, organization.id);
                assert_eq!(found_chapter, chapter);
            }
        }
    }

    #[test]
    fn public_threat_referral_capability_is_explicitly_authored_for_three_roles() {
        for organization in &catalog().organizations {
            let expected = organization.starting_role.as_ref().is_some_and(|role| {
                matches!(
                    role.profession,
                    StartingProfession::Forester
                        | StartingProfession::WitchHunter
                        | StartingProfession::Knight
                )
            });
            assert_eq!(
                organization.public_threat_referrals, expected,
                "{}",
                organization.id
            );
        }
    }

    #[test]
    fn only_the_order_of_saint_george_can_issue_errantry() {
        for organization in &catalog().organizations {
            assert_eq!(
                organization.errantry_issuance,
                organization.id == "order_saint_george",
                "{}",
                organization.id
            );
        }
    }

    #[test]
    fn exact_representative_requires_identity_home_and_conversation() {
        let valid = (
            9_007_199_254_740_993,
            9_007_199_254_740_993,
            "goslar",
            "goslar",
            "ranger_lodge",
            "ranger_lodge",
            "organization-representative",
        );
        assert!(exact_representative_fields_match(
            valid.0, valid.1, valid.2, valid.3, valid.4, valid.5, valid.6
        ));
        assert!(!exact_representative_fields_match(
            9_007_199_254_740_994,
            valid.1,
            valid.2,
            valid.3,
            valid.4,
            valid.5,
            valid.6,
        ));
        assert!(!exact_representative_fields_match(
            valid.0, valid.1, "other", valid.3, valid.4, valid.5, valid.6
        ));
        assert!(!exact_representative_fields_match(
            valid.0, valid.1, valid.2, valid.3, "other", valid.5, valid.6
        ));
        assert!(!exact_representative_fields_match(
            valid.0,
            valid.1,
            valid.2,
            valid.3,
            valid.4,
            valid.5,
            "innkeeper",
        ));
    }

    #[test]
    fn service_chapters_have_stable_physical_location_mappings() {
        assert_eq!(service_npc_location_id("merchants"), Some("market"));
        assert_eq!(service_npc_location_id("weapons"), Some("forge"));
        assert_eq!(service_npc_location_id("armor"), Some("armoury"));
        assert_eq!(service_npc_location_id("clothing"), Some("tailor"));
        assert_eq!(service_npc_location_id("books"), Some("bookstore"));
        assert_eq!(service_npc_location_id("herbalist"), Some("herbalist"));
        assert_eq!(service_npc_location_id("inn"), Some("inn"));
        assert_eq!(service_npc_location_id("religion"), Some("church"));
        assert_eq!(service_npc_location_id("physician"), None);
        assert_eq!(service_npc_location_id("surgeon"), None);
    }

    #[test]
    fn chapter_location_is_colocated_only_when_the_mapped_service_is_available() {
        use adventuresim_world_schema::{SettlementEconomyProfile, SettlementService};

        let merchant = organization("merchant_guild").unwrap();
        let merchant_chapter = merchant.chapter("viabundus-0").unwrap();
        let mut profile = SettlementEconomyProfile::stage_placeholder();
        profile.services = vec![SettlementService::Market];
        assert_eq!(
            chapter_effective_location_id(merchant, merchant_chapter, &profile),
            "market"
        );
        assert!(!chapter_has_standalone_building(
            merchant,
            merchant_chapter,
            &profile
        ));

        profile.services.clear();
        assert_eq!(
            chapter_effective_location_id(merchant, merchant_chapter, &profile),
            merchant_chapter.location_id
        );
        assert!(chapter_has_standalone_building(
            merchant,
            merchant_chapter,
            &profile
        ));

        let physicians = organization("physicians_college").unwrap();
        let physician_chapter = physicians.chapter("viabundus-0").unwrap();
        profile.services = vec![SettlementService::Market, SettlementService::Temple];
        assert_eq!(
            chapter_effective_location_id(physicians, physician_chapter, &profile),
            physician_chapter.location_id
        );

        for id in ["hunt_pale_lantern", "order_saint_george", "lodge_hart_king"] {
            let definition = organization(id).unwrap();
            let chapter = &definition.chapters[0];
            assert!(definition.service_id.is_none());
            assert!(chapter_has_standalone_building(
                definition, chapter, &profile
            ));
        }
    }

    #[test]
    fn representative_ids_are_location_independent_and_organization_unique() {
        let merchant = organization_representative_id("viabundus-0", "merchant_guild");
        assert_eq!(
            merchant,
            organization_representative_id("viabundus-0", "merchant_guild")
        );
        assert_ne!(
            merchant,
            organization_representative_id("viabundus-0", "weaponsmith_guild")
        );
        assert_ne!(merchant, 0);
        assert!(merchant >= 1u64 << 63);
    }
}
