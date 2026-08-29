//! Strategic-web boundary rows and presentation projections.
//!
//! Exact SpacetimeDB rows are generated from the module schema. The structs
//! defined here are deliberately narrower presentation or joined-query views;
//! none duplicate a persisted row.

pub use adventuresim_core::{
    capability::RoleRequirements,
    investigation_action::{InvestigationActionAvailability, InvestigationActionUnavailableReason},
    item_catalog::{
        EquipmentBodyPart, EquipmentChannel, EquipmentLocation, OccupancyRequirement,
        ParentRequirement, Slot,
    },
    personality::{
        Conscience, Conviction, Courtship, Drive, Hygiene, Inclination, Mirth, Nerve, Outlook,
        Personality, Presentation, SelfKnowledge, SelfRegard, Sex, Sociability, Temperance,
        Transparency,
    },
    physiology::BodyRegion,
    strategic_place::CaseSiteId,
};
use adventuresim_core::{
    combat_style::MeleeAttackStyle, equipment::WeaponSkillDistribution,
    physiology::InterventionRoute, social::PersonalityAxis,
};
use adventuresim_stdb_client as sats;
pub use adventuresim_stdb_client::{
    AffinityBand, AlcoholConsumption, AutomaticSocialChat, AutoresolveReport,
    BackendBestiaryDeduction, BackendBrowserCharacterAccess, BackendCaseSitePin, BackendChallenge,
    BackendCharacterCaseSiteLocation, BackendCharacterRelationshipStatus,
    BackendCharacterResidenceStatus, BackendContextCharacter, BackendContextualDecision,
    BackendContract, BackendCorpse, BackendDevelopmentQuest, BackendDevelopmentScenario,
    BackendDialogueEvent, BackendDialogueParticipant, BackendDialoguePrompt,
    BackendDialogueSession, BackendDialogueTopicOption, BackendDialogueWitnessClaim,
    BackendFamilyChild, BackendFireplaceDish, BackendFireplaceStation, BackendForageAttemptState,
    BackendForageReceipt, BackendHostileNegotiation, BackendHostileSurrender,
    BackendIngredientPreparationPlan, BackendInvestigationAction, BackendInvestigationCaseSummary,
    BackendInvestigationJournalEntry, BackendInvestigationLead, BackendLocalChatMessage,
    BackendLocalProblemTradeEffect, BackendOrganizationMembership, BackendPhysicalEvidence,
    BackendPhysicalEvidenceInspection, BackendPhysiologyAdministration, BackendPhysiologyChart,
    BackendRoadChallenge, BackendSettlementResident, BackendSettlementResidentRelationship,
    BackendSocialChatReceipt, BackendTinctureStatus, BattleLootItem, BattleResult,
    ChallengePresenterCatalogId, CharacterAffinity, CharacterAttributes, CharacterCapability,
    CharacterCondition, CharacterDeath, CharacterFamiliarity, CharacterFilth, CharacterLimbs,
    CharacterMoraleSource, CharacterNeeds, CharacterSettlementReputation, CharacterSkills,
    CharacterStats, CharacterStrategicCondition, CharacterTime, CharacterTrainingSchedule,
    ChildActivityFocus, ChildStage, ContainerLiquid, ContractStatus, CourtshipKind,
    DestinationKnowledgeStage, EquipmentAnchorKind, EquipmentOccupancy, FamiliarityBand, FoodLot,
    HostileSurrenderMode, HousingTier, IngredientPreparationAction, InventoryContainment,
    InventoryItem, InventoryItemAmount, InventoryLocation, InventoryObject,
    InventoryQuantityTarget, ItemCondition, JourneyCampInterval, JourneyEndpoint,
    JourneyPrecipitation, JourneyRouteLeg, JourneyRoutePoint, JourneyTerrainKind,
    JourneyTerrainSpan, LimbInjury, MoraleBand, NpcAgeBand, NpcPresentation,
    OrganizationMembershipStatus, OrganizationPresentation, PartyInventoryItem, PartyItemAmount,
    PartyJoinRequest, PartyJourney, PartyLeaderVote, PartyMember, PartyStake, ProjectileKind,
    RecruitmentOffer, RecruitmentOfferStatus, ReligiousDemand, RepairOrder, ResidenceTenure,
    RetainedProjectile, SavedRecruitmentRole, ScheduleAllocation, SettlementAlias,
    SettlementCategory, SettlementDescription, SettlementDescriptionKind, SettlementResidenceOffer,
    SettlementResidentPresence, SettlementSmith, SocialAddress, SocialBelief, SocialChatOutcome,
    SocialChatTargetKind, StrategicEncounter, StrategicEncounterStatus, WeaponHolderInstance,
    WeaponInstance, WorldClock,
};
#[cfg(test)]
pub use adventuresim_stdb_client::{
    BackendPhysiologyDifferential, FilthOrigin, FilthSubstance, FoodPreparation,
    JourneyCaseSiteEndpoint, JourneySettlementEndpoint, StrategicEncounterLoss,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use spacetimedb_sats::{ser::Serialize as SatsSerialize, serde::SerdeWrapper};

pub type QueryResponse = Vec<QueryResult>;

#[derive(Debug, Deserialize)]
pub struct QueryResult {
    pub schema: QuerySchema,
    pub rows: Vec<Value>,
}

#[derive(Debug, Deserialize)]
pub struct QuerySchema {
    pub elements: Vec<SchemaElement>,
}

#[derive(Debug, Deserialize)]
pub struct SchemaElement {
    pub name: Option<AlgebraicTypeRef>,
    pub algebraic_type: AlgebraicType,
}

#[derive(Debug, Deserialize)]
pub struct AlgebraicTypeRef {
    pub some: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum AlgebraicType {
    Value(Value),
}

pub(crate) const fn npc_age_band_id(value: NpcAgeBand) -> &'static str {
    match value {
        NpcAgeBand::Child => "child",
        NpcAgeBand::Adolescent => "adolescent",
        NpcAgeBand::Adult => "adult",
        NpcAgeBand::Elder => "elder",
    }
}

pub(crate) const fn npc_presentation_id(value: NpcPresentation) -> &'static str {
    match value {
        NpcPresentation::Man => "man",
        NpcPresentation::Ambiguous => "ambiguous",
        NpcPresentation::Woman => "woman",
    }
}

fn sats_to_serde<T, U>(value: &T) -> serde_json::Result<U>
where
    T: SatsSerialize + ?Sized,
    U: DeserializeOwned,
{
    serde_json::from_value(normalize_sats_serde_value(serde_json::to_value(
        SerdeWrapper::from_ref(value),
    )?))
}

fn normalize_sats_serde_value(value: Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.into_iter().map(normalize_sats_serde_value).collect())
        }
        Value::Object(object) if object.len() == 1 => {
            let (name, payload) = object.into_iter().next().expect("one field");
            if name.eq_ignore_ascii_case("none") && payload.as_array().is_some_and(Vec::is_empty) {
                return Value::Null;
            }
            if name.eq_ignore_ascii_case("some") {
                return normalize_sats_serde_value(payload);
            }
            if payload.as_array().is_some_and(Vec::is_empty) {
                return Value::String(name);
            }
            Value::Object(
                [(name, normalize_sats_serde_value(payload))]
                    .into_iter()
                    .collect(),
            )
        }
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(name, value)| (name, normalize_sats_serde_value(value)))
                .collect(),
        ),
        value => value,
    }
}

fn core_official_religion(
    value: sats::OfficialReligion,
) -> adventuresim_world_schema::OfficialReligion {
    match value {
        sats::OfficialReligion::RomanCatholic => {
            adventuresim_world_schema::OfficialReligion::RomanCatholic
        }
        sats::OfficialReligion::Lutheran => adventuresim_world_schema::OfficialReligion::Lutheran,
        sats::OfficialReligion::Reformed => adventuresim_world_schema::OfficialReligion::Reformed,
        sats::OfficialReligion::Anglican => adventuresim_world_schema::OfficialReligion::Anglican,
        sats::OfficialReligion::EasternOrthodox => {
            adventuresim_world_schema::OfficialReligion::EasternOrthodox
        }
        sats::OfficialReligion::Islamic => adventuresim_world_schema::OfficialReligion::Islamic,
        sats::OfficialReligion::Judaism => adventuresim_world_schema::OfficialReligion::Judaism,
    }
}

fn core_western_christian_arrangement(
    value: sats::WesternChristianArrangement,
) -> adventuresim_world_schema::WesternChristianArrangement {
    match value {
        sats::WesternChristianArrangement::CatholicLutheran(church) => {
            adventuresim_world_schema::WesternChristianArrangement::CatholicLutheran {
                church: match church {
                    sats::CatholicLutheranChurch::RomanCatholic => {
                        adventuresim_world_schema::CatholicLutheranChurch::RomanCatholic
                    }
                    sats::CatholicLutheranChurch::Lutheran => {
                        adventuresim_world_schema::CatholicLutheranChurch::Lutheran
                    }
                },
            }
        }
        sats::WesternChristianArrangement::CatholicReformed(church) => {
            adventuresim_world_schema::WesternChristianArrangement::CatholicReformed {
                church: match church {
                    sats::CatholicReformedChurch::RomanCatholic => {
                        adventuresim_world_schema::CatholicReformedChurch::RomanCatholic
                    }
                    sats::CatholicReformedChurch::Reformed => {
                        adventuresim_world_schema::CatholicReformedChurch::Reformed
                    }
                },
            }
        }
        sats::WesternChristianArrangement::LutheranReformed(church) => {
            adventuresim_world_schema::WesternChristianArrangement::LutheranReformed {
                church: match church {
                    sats::LutheranReformedChurch::Lutheran => {
                        adventuresim_world_schema::LutheranReformedChurch::Lutheran
                    }
                    sats::LutheranReformedChurch::Reformed => {
                        adventuresim_world_schema::LutheranReformedChurch::Reformed
                    }
                },
            }
        }
    }
}

fn core_settlement_religious_status(
    value: sats::SettlementReligiousStatus,
) -> adventuresim_world_schema::SettlementReligiousStatus {
    match value {
        sats::SettlementReligiousStatus::Established(religion) => {
            adventuresim_world_schema::SettlementReligiousStatus::Established {
                religion: core_official_religion(religion),
            }
        }
        sats::SettlementReligiousStatus::Parity(arrangement) => {
            adventuresim_world_schema::SettlementReligiousStatus::Parity {
                arrangement: core_western_christian_arrangement(arrangement),
            }
        }
        sats::SettlementReligiousStatus::MultiConfessional(arrangement) => {
            adventuresim_world_schema::SettlementReligiousStatus::MultiConfessional {
                arrangement: core_western_christian_arrangement(arrangement),
            }
        }
        sats::SettlementReligiousStatus::LocallyDetermined(church) => {
            adventuresim_world_schema::SettlementReligiousStatus::LocallyDetermined {
                church: core_official_religion(church),
            }
        }
    }
}

fn sql_unit_variant_name<E: serde::de::Error>(value: Value) -> Result<String, E> {
    match value {
        Value::String(name) => Ok(name),
        Value::Object(variant) if variant.len() == 1 => {
            Ok(variant.into_iter().next().expect("one variant").0)
        }
        _ => Err(E::custom("expected a unit enum variant")),
    }
}

fn serialize_settlement_category<S>(
    value: &SettlementCategory,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(match value {
        SettlementCategory::Unknown => "Unknown",
        SettlementCategory::Hamlet => "Hamlet",
        SettlementCategory::Village => "Village",
        SettlementCategory::Town => "Town",
        SettlementCategory::City => "City",
        SettlementCategory::Capital => "Capital",
    })
}

fn deserialize_settlement_category<'de, D>(deserializer: D) -> Result<SettlementCategory, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match sql_unit_variant_name::<D::Error>(Value::deserialize(deserializer)?)?.as_str() {
        "Unknown" => Ok(SettlementCategory::Unknown),
        "Hamlet" => Ok(SettlementCategory::Hamlet),
        "Village" => Ok(SettlementCategory::Village),
        "Town" => Ok(SettlementCategory::Town),
        "City" => Ok(SettlementCategory::City),
        "Capital" => Ok(SettlementCategory::Capital),
        _ => Err(serde::de::Error::custom("unknown settlement category")),
    }
}

fn deserialize_equipment_channel<'de, D>(deserializer: D) -> Result<EquipmentChannel, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match sql_unit_variant_name::<D::Error>(Value::deserialize(deserializer)?)?.as_str() {
        "Held" => Ok(EquipmentChannel::Held),
        "BaseClothing" => Ok(EquipmentChannel::BaseClothing),
        "Padding" => Ok(EquipmentChannel::Padding),
        "FlexibleArmor" => Ok(EquipmentChannel::FlexibleArmor),
        "RigidArmor" => Ok(EquipmentChannel::RigidArmor),
        "Outerwear" => Ok(EquipmentChannel::Outerwear),
        "Accessory" => Ok(EquipmentChannel::Accessory),
        "Mount" => Ok(EquipmentChannel::Mount),
        "Containment" => Ok(EquipmentChannel::Containment),
        _ => Err(serde::de::Error::custom("unknown equipment channel")),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BestiaryEnemyLoreView {
    pub id: String,
    pub name: String,
    pub is_primary: bool,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
}

pub fn bestiary_enemy_lore(
    category: adventuresim_world_schema::BestiaryCategory,
) -> Vec<BestiaryEnemyLoreView> {
    adventuresim_core::bestiary::profiles_for_category(category)
        .into_iter()
        .map(|categorized| {
            let profile = categorized.profile;
            let lore = adventuresim_core::bestiary::implemented_combat_lore(profile);
            BestiaryEnemyLoreView {
                id: profile.id.as_str().into(),
                name: profile.display_name.into(),
                is_primary: categorized.is_primary,
                strengths: lore.strengths,
                weaknesses: lore.weaknesses,
            }
        })
        .collect()
}

pub trait BestiaryDeductionExt {
    fn provenance(&self) -> Vec<String>;
}

impl BestiaryDeductionExt for BackendBestiaryDeduction {
    fn provenance(&self) -> Vec<String> {
        serde_json::from_str::<Vec<String>>(&self.provenance_json)
            .unwrap_or_default()
            .into_iter()
            .filter(|item| !item.trim().is_empty() && item.len() <= 1_024)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CharacterView {
    pub id: u64,
    pub name: String,
    pub xp: u32,
    pub level: u32,
    pub current_settlement_id: Option<String>,
    pub current_case_site_id: Option<CaseSiteId>,
    pub party_id: Option<String>,
    pub age_years: u16,
    pub alive: bool,
    pub temporary: bool,
    pub social_notification_count: usize,
    pub automatic_social_chat_enabled: bool,
}

impl From<sats::Character> for CharacterView {
    fn from(row: sats::Character) -> Self {
        let sats::Character {
            id,
            scan_id: _,
            name,
            xp,
            level,
            current_settlement_id,
            party_id,
            server: _,
            in_server: _,
            temporary,
            age_years,
            alive,
            party_treatment_decision: _,
        } = row;
        Self {
            id,
            name,
            xp,
            level,
            current_settlement_id,
            current_case_site_id: None,
            party_id,
            age_years,
            alive,
            temporary,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettlementView {
    pub id: String,
    pub name: String,
    pub longitude: f64,
    pub latitude: f64,
    pub population_level: i32,
    pub population_estimate: u32,
    #[serde(
        serialize_with = "serialize_settlement_category",
        deserialize_with = "deserialize_settlement_category"
    )]
    pub category: SettlementCategory,
    pub languages: adventuresim_world_schema::SettlementLanguageProfile,
    pub industries: adventuresim_world_schema::InferredIndustryProfile,
    pub economy: adventuresim_world_schema::SettlementEconomyProfile,
    pub religious_status: adventuresim_world_schema::SettlementReligiousStatus,
    pub scene_key: String,
    pub religion_id: String,
    pub currency_id: String,
    pub source_node_id: Option<u64>,
}

impl TryFrom<sats::Settlement> for SettlementView {
    type Error = serde_json::Error;

    fn try_from(row: sats::Settlement) -> Result<Self, Self::Error> {
        let sats::Settlement {
            id,
            name,
            coord_x,
            coord_y,
            population_level,
            population_estimate,
            category,
            elevation: _,
            land_use: _,
            forest_cover: _,
            potential_vegetation: _,
            historical_vegetation: _,
            tree_species: _,
            soil: _,
            geology: _,
            religious_status,
            languages,
            drought: _,
            hydrology: _,
            industries,
            economy,
            scene_key,
            religion_id,
            currency_id,
            source_node_id,
            sources: _,
        } = row;
        Ok(Self {
            id,
            name,
            longitude: coord_x,
            latitude: coord_y,
            population_level,
            population_estimate,
            category,
            languages: sats_to_serde(&languages)?,
            industries: sats_to_serde(&industries)?,
            economy: sats_to_serde(&economy)?,
            religious_status: core_settlement_religious_status(religious_status),
            scene_key,
            religion_id,
            currency_id,
            source_node_id,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TravelEdgeView {
    pub id: u64,
    pub from_node_id: u64,
    pub to_node_id: u64,
    pub route: adventuresim_world_schema::TravelRoute,
    pub length_m: u32,
    pub slope_multiplier: f32,
    pub terrain: adventuresim_world_schema::RouteTerrain,
    pub certainty: u8,
    pub section: String,
}

impl TryFrom<sats::TravelEdge> for TravelEdgeView {
    type Error = serde_json::Error;

    fn try_from(row: sats::TravelEdge) -> Result<Self, Self::Error> {
        let sats::TravelEdge {
            id,
            from_node_id,
            to_node_id,
            route,
            provenance: _,
            toll_at: _,
            length_m,
            slope_multiplier,
            terrain,
            certainty,
            section,
            sources: _,
        } = row;
        Ok(Self {
            id,
            from_node_id,
            to_node_id,
            route: sats_to_serde(&route)?,
            length_m,
            slope_multiplier,
            terrain: sats_to_serde(&terrain)?,
            certainty,
            section,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseBattleView {
    pub gateway_bucket: u8,
    pub owner_character_id: u64,
    pub public_case_id: String,
    pub party_id: String,
    pub battle_id: String,
    pub mission_id: String,
    pub case_site_id: CaseSiteId,
}

impl TryFrom<sats::BackendCaseBattle> for CaseBattleView {
    type Error = adventuresim_core::strategic_place::PlaceIdentityError;

    fn try_from(row: sats::BackendCaseBattle) -> Result<Self, Self::Error> {
        let sats::BackendCaseBattle {
            gateway_bucket,
            owner_character_id,
            public_case_id,
            party_id,
            battle_id,
            mission_id,
            case_site_id,
        } = row;
        Ok(Self {
            gateway_bucket,
            owner_character_id,
            public_case_id,
            party_id,
            battle_id,
            mission_id,
            case_site_id: CaseSiteId::try_new(case_site_id.value)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartyView {
    pub id: String,
    pub gateway_bucket: u8,
    pub name: String,
    pub leader_id: u64,
    pub current_settlement_id: Option<String>,
    pub current_case_site_id: Option<CaseSiteId>,
    pub active_contract_id: Option<String>,
    pub is_solo: bool,
    pub camp_fatigue_percent: u8,
    pub walking_minutes_per_day: u16,
    pub travel_at_night: bool,
    pub journey_start_minute_of_day: u16,
    pub wilderness_canonical_anchor_minute: Option<u64>,
    pub wilderness_elapsed_minutes: u64,
    pub camp_destination: Option<JourneyEndpoint>,
    pub camp_remaining_minutes: u64,
    pub physiology_target: f32,
    pub command_target: f32,
    pub religion_target: f32,
}

impl TryFrom<sats::Party> for PartyView {
    type Error = adventuresim_core::strategic_place::PlaceIdentityError;

    fn try_from(row: sats::Party) -> Result<Self, Self::Error> {
        let sats::Party {
            id,
            gateway_bucket,
            name,
            leader_id,
            current_settlement_id,
            current_case_site_id,
            active_contract_id,
            is_solo,
            camp_fatigue_percent,
            walking_minutes_per_day,
            travel_at_night,
            journey_start_minute_of_day,
            wilderness_canonical_anchor_minute,
            wilderness_elapsed_minutes,
            camp_destination,
            camp_remaining_minutes,
            physiology_target,
            command_target,
            religion_target,
        } = row;
        Ok(Self {
            id,
            gateway_bucket,
            name,
            leader_id,
            current_settlement_id,
            current_case_site_id: current_case_site_id
                .map(|site| CaseSiteId::try_new(site.value))
                .transpose()?,
            active_contract_id,
            is_solo,
            camp_fatigue_percent,
            walking_minutes_per_day,
            travel_at_night,
            journey_start_minute_of_day,
            wilderness_canonical_anchor_minute,
            wilderness_elapsed_minutes,
            camp_destination,
            camp_remaining_minutes,
            physiology_target,
            command_target,
            religion_target,
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PartyActionRequestView {
    pub id: u64,
    pub gateway_bucket: u8,
    pub party_id: String,
    pub requester_id: u64,
    pub action_kind: String,
    pub summary: String,
    pub payload: String,
}

impl From<sats::PartyActionRequest> for PartyActionRequestView {
    fn from(row: sats::PartyActionRequest) -> Self {
        let sats::PartyActionRequest {
            id,
            gateway_bucket,
            party_id,
            requester_id,
            action_kind,
            summary,
            payload,
        } = row;
        Self {
            id,
            gateway_bucket,
            party_id,
            requester_id,
            action_kind,
            summary,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartyJourneyRouteView {
    pub party_id: String,
    pub gateway_bucket: u8,
    pub package_digest: String,
    pub weather_rules_version: u16,
    pub weather_interval_start: u64,
    pub precipitation: JourneyPrecipitation,
    pub intensity_bps: u16,
    pub ground_moisture_bps: u16,
    pub snow_cover_bps: u16,
    pub distance_m: u64,
    pub minutes: u64,
    pub points: Vec<JourneyRoutePoint>,
    pub spans: Vec<JourneyTerrainSpan>,
    pub return_route: Option<JourneyRouteLeg>,
}

impl From<sats::PartyJourneyRoute> for PartyJourneyRouteView {
    fn from(row: sats::PartyJourneyRoute) -> Self {
        let sats::PartyJourneyRoute {
            party_id,
            gateway_bucket,
            package_digest,
            weather_rules_version,
            weather_interval_start,
            precipitation,
            intensity_bps,
            ground_moisture_bps,
            snow_cover_bps,
            distance_m,
            minutes,
            points,
            spans,
            return_route,
        } = row;
        Self {
            party_id,
            gateway_bucket,
            package_digest,
            weather_rules_version,
            weather_interval_start,
            precipitation,
            intensity_bps,
            ground_moisture_bps,
            snow_cover_bps,
            distance_m,
            minutes,
            points,
            spans,
            return_route,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EquippedItemView {
    pub inventory_item_id: u64,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the generated-row boundary retains the owning character for explicit projection"
        )
    )]
    pub character_id: u64,
    pub placement_id: String,
    pub item_name: String,
}

impl From<sats::CharacterEquippedItem> for EquippedItemView {
    fn from(row: sats::CharacterEquippedItem) -> Self {
        let sats::CharacterEquippedItem {
            inventory_item_id,
            character_id,
            placement_id,
        } = row;
        Self {
            inventory_item_id,
            character_id,
            placement_id,
            item_name: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecruitmentRoleView {
    pub id: u64,
    pub party_id: String,
    pub purpose: sats::RecruitmentRolePurpose,
    pub name: String,
    pub requirements: RoleRequirements,
    pub quantity: u32,
    pub autoresolve_combat_power: u64,
}

impl From<sats::PartyRecruitmentRole> for RecruitmentRoleView {
    fn from(row: sats::PartyRecruitmentRole) -> Self {
        let sats::PartyRecruitmentRole {
            id,
            party_id,
            purpose,
            name,
            requirements,
            quantity,
        } = row;
        Self {
            id,
            party_id,
            purpose,
            name,
            requirements: role_requirements(&requirements),
            quantity,
            autoresolve_combat_power: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MissionStatus {
    #[default]
    Ready,
    Pending,
    Failed,
    Ended,
}

impl MissionStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Pending => "Pending",
            Self::Failed => "Failed",
            Self::Ended => "Ended",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MissionServerView {
    pub identity: Option<String>,
    pub gateway_bucket: u8,
    pub mission_id: String,
    pub scene_key: String,
    pub party_id: String,
    pub status: MissionStatus,
    pub addr: String,
    pub cert_digest: String,
    pub character_id: Option<u64>,
}

impl MissionServerView {
    pub fn pending(
        mission_id: String,
        gateway_bucket: u8,
        scene_key: String,
        party_id: String,
    ) -> Self {
        Self {
            identity: None,
            gateway_bucket,
            mission_id,
            scene_key,
            party_id,
            status: MissionStatus::Pending,
            addr: String::new(),
            cert_digest: String::new(),
            character_id: None,
        }
    }
}

impl From<sats::TacticalServer> for MissionServerView {
    fn from(row: sats::TacticalServer) -> Self {
        let sats::TacticalServer {
            identity,
            gateway_bucket,
            mission_id,
            scene_key,
            party_id,
            addr,
            cert_digest,
            expected_party_members: _,
            authorized_party_member_ids: _,
            required_enemy_kills: _,
            enemy_difficulty: _,
            enemy_combat_scale_bps: _,
            countermeasure_multiplier_bps: _,
            normalized_combat_power: _,
            enemy_character_ids: _,
            party_has_surprise: _,
        } = row;
        Self {
            identity: Some(identity.to_string()),
            gateway_bucket,
            mission_id,
            scene_key,
            party_id,
            status: MissionStatus::Ready,
            addr,
            cert_digest,
            character_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MissionServerRequestView {
    pub mission_id: String,
    pub gateway_bucket: u8,
    pub scene_key: String,
    pub party_id: String,
    pub requested_by: u64,
    pub required_enemy_kills: u32,
}

impl From<sats::TacticalServerRequest> for MissionServerRequestView {
    fn from(row: sats::TacticalServerRequest) -> Self {
        let sats::TacticalServerRequest {
            mission_id,
            gateway_bucket,
            scene_key,
            party_id,
            requested_by,
            longitude_e_7: _,
            latitude_e_7: _,
            absolute_minute: _,
            lunar_phase_minute: _,
            expected_party_members: _,
            authorized_party_member_ids: _,
            required_enemy_kills,
            enemy_difficulty: _,
            enemy_combat_scale_bps: _,
            countermeasure_multiplier_bps: _,
            normalized_combat_power: _,
            enemy_character_ids: _,
            party_has_surprise: _,
        } = row;
        Self {
            mission_id,
            gateway_bucket,
            scene_key,
            party_id,
            requested_by,
            required_enemy_kills,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CharacterEquipmentGraph {
    pub _character_id: u64,
    pub worn_item_ids: Vec<u64>,
    pub equipment_nodes: Vec<EquippedItemView>,
    pub equipment_occupancies: Vec<EquipmentOccupancy>,
    pub attachment_targets: Vec<EquipmentAttachmentTarget>,
}

impl CharacterEquipmentGraph {
    pub fn contains(&self, inventory_item_id: u64) -> bool {
        self.worn_item_ids.contains(&inventory_item_id)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentAttachmentTarget {
    pub parent_inventory_item_id: u64,
    pub parent_item_name: String,
    pub attachment_point_id: String,
    #[serde(deserialize_with = "deserialize_equipment_channel")]
    pub channel: EquipmentChannel,
    pub accepts_tags: Vec<String>,
    pub free_capacity: u16,
    pub order: u16,
}

#[derive(Debug, Clone)]
pub struct CatalogEquipmentPlacement {
    pub id: String,
    pub occupancy: Vec<OccupancyRequirement>,
    pub parents: Vec<ParentRequirement>,
    pub protection: Vec<EquipmentBodyPart>,
}

#[derive(Debug, Clone)]
pub struct CatalogEquipmentAttachmentPoint {
    pub id: String,
    pub channel: EquipmentChannel,
    pub capacity: u16,
    pub order: u16,
    pub accepts_tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogItemKind {
    Simple,
    Weapon,
    Armor,
    Shield,
    Clothing,
    Container,
    Currency,
    Ingredient,
    Medication,
    Food,
}

#[derive(Debug, Clone)]
pub struct CatalogItemView {
    pub id: String,
    pub weight: f32,
    pub exterior_volume_ml: u32,
    pub slot: Slot,
    pub kind: CatalogItemKind,
    pub equipment_placements: Vec<CatalogEquipmentPlacement>,
    pub attachment_tags: Vec<String>,
    pub attachment_points: Vec<CatalogEquipmentAttachmentPoint>,
    pub repairable: bool,
    pub accuracy: f32,
    pub swing_precision: f32,
    pub stab_precision: f32,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the catalog projection records persisted combat fields even before presentation consumes them"
        )
    )]
    pub preferred_melee_style: MeleeAttackStyle,
    pub reach: f32,
    pub block: f32,
    pub coverage: f32,
    pub penetration: f32,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
    pub range_of_motion: f32,
    pub precise: bool,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the catalog projection records persisted combat fields even before presentation consumes them"
        )
    )]
    pub moment_of_inertia_kg_m_2: f32,
    pub balance: f32,
    pub melee: bool,
    pub ranged: bool,
    pub weapon_skills: WeaponSkillDistribution,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub base_value: Option<u32>,
    pub nutrition_kcal: f32,
    pub water_capacity_ml: u32,
    pub container_capacity_ml: u32,
    pub alcohol_serving_ml: u32,
    pub alcohol_abv_basis_points: u16,
    pub alcohol_net_hydration_ml: u32,
    pub alcohol_disinfectant_effectiveness: u16,
    pub alcohol_disinfectant_focused: bool,
    pub alcohol_potable: bool,
    pub quality: u8,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the catalog projection records persisted durability fields even before presentation consumes them"
        )
    )]
    pub durability_yield: f32,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the catalog projection records persisted durability fields even before presentation consumes them"
        )
    )]
    pub durability_fracture: f32,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the catalog projection records persisted durability fields even before presentation consumes them"
        )
    )]
    pub durability_wear: f32,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the catalog projection records persisted durability fields even before presentation consumes them"
        )
    )]
    pub durability_failure_share: f32,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the catalog projection records persisted durability fields even before presentation consumes them"
        )
    )]
    pub edge_sensitivity: f32,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the catalog projection records persisted durability fields even before presentation consumes them"
        )
    )]
    pub handling_sensitivity: f32,
}

impl From<sats::Item> for CatalogItemView {
    fn from(row: sats::Item) -> Self {
        let sats::Item {
            id,
            weight,
            exterior_volume_ml,
            slot,
            kind,
            equipment_placements,
            attachment_tags,
            attachment_points,
            repairable,
            accuracy,
            swing_precision,
            stab_precision,
            preferred_melee_style,
            reach,
            block,
            coverage,
            penetration,
            resistance,
            padding,
            flexibility,
            range_of_motion,
            precise,
            moment_of_inertia_kg_m_2,
            balance,
            melee,
            ranged,
            weapon_skills,
            blunt,
            slash,
            pierce,
            base_value,
            nutrition_kcal,
            water_capacity_ml,
            container_capacity_ml,
            alcohol_serving_ml,
            alcohol_abv_basis_points,
            alcohol_net_hydration_ml,
            alcohol_disinfectant_effectiveness,
            alcohol_disinfectant_focused,
            alcohol_potable,
            quality,
            durability_yield,
            durability_fracture,
            durability_wear,
            durability_failure_share,
            edge_sensitivity,
            handling_sensitivity,
        } = row;
        Self {
            id,
            weight,
            exterior_volume_ml,
            slot: core_slot(slot),
            kind: catalog_item_kind(kind),
            equipment_placements: equipment_placements
                .into_iter()
                .map(|placement| {
                    let sats::PersistedEquipmentPlacement {
                        id,
                        occupancy,
                        parents,
                        protection,
                    } = placement;
                    CatalogEquipmentPlacement {
                        id,
                        occupancy: occupancy
                            .into_iter()
                            .map(|requirement| {
                                let sats::OccupancyRequirement {
                                    location,
                                    channel,
                                    order,
                                } = requirement;
                                OccupancyRequirement {
                                    location: core_equipment_location(location),
                                    channel: core_equipment_channel(channel),
                                    order,
                                }
                            })
                            .collect(),
                        parents: parents
                            .into_iter()
                            .map(|requirement| {
                                let sats::ParentRequirement { channel, order } = requirement;
                                ParentRequirement {
                                    channel: core_equipment_channel(channel),
                                    order,
                                }
                            })
                            .collect(),
                        protection: protection
                            .into_iter()
                            .map(core_equipment_body_part)
                            .collect(),
                    }
                })
                .collect(),
            attachment_tags,
            attachment_points: attachment_points
                .into_iter()
                .map(|point| {
                    let sats::PersistedEquipmentAttachmentPoint {
                        id,
                        channel,
                        capacity,
                        order,
                        accepts_tags,
                    } = point;
                    CatalogEquipmentAttachmentPoint {
                        id,
                        channel: core_equipment_channel(channel),
                        capacity,
                        order,
                        accepts_tags,
                    }
                })
                .collect(),
            repairable,
            accuracy,
            swing_precision,
            stab_precision,
            preferred_melee_style: core_melee_style(preferred_melee_style),
            reach,
            block,
            coverage,
            penetration,
            resistance,
            padding,
            flexibility,
            range_of_motion,
            precise,
            moment_of_inertia_kg_m_2,
            balance,
            melee,
            ranged,
            weapon_skills: core_weapon_skills(weapon_skills),
            blunt,
            slash,
            pierce,
            base_value,
            nutrition_kcal,
            water_capacity_ml,
            container_capacity_ml,
            alcohol_serving_ml,
            alcohol_abv_basis_points,
            alcohol_net_hydration_ml,
            alcohol_disinfectant_effectiveness,
            alcohol_disinfectant_focused,
            alcohol_potable,
            quality,
            durability_yield,
            durability_fracture,
            durability_wear,
            durability_failure_share,
            edge_sensitivity,
            handling_sensitivity,
        }
    }
}

impl Default for CatalogItemView {
    fn default() -> Self {
        Self {
            id: String::new(),
            weight: 0.0,
            exterior_volume_ml: 0,
            slot: Slot::None,
            kind: CatalogItemKind::Simple,
            equipment_placements: Vec::new(),
            attachment_tags: Vec::new(),
            attachment_points: Vec::new(),
            repairable: false,
            accuracy: 0.0,
            swing_precision: 0.0,
            stab_precision: 0.0,
            preferred_melee_style: MeleeAttackStyle::default(),
            reach: 0.0,
            block: 0.0,
            coverage: 0.0,
            penetration: 0.0,
            resistance: 0.0,
            padding: 0.0,
            flexibility: 0.0,
            range_of_motion: 0.0,
            precise: false,
            moment_of_inertia_kg_m_2: 0.0,
            balance: 0.0,
            melee: false,
            ranged: false,
            weapon_skills: WeaponSkillDistribution::default(),
            blunt: false,
            slash: false,
            pierce: false,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            container_capacity_ml: 0,
            alcohol_serving_ml: 0,
            alcohol_abv_basis_points: 0,
            alcohol_net_hydration_ml: 0,
            alcohol_disinfectant_effectiveness: 0,
            alcohol_disinfectant_focused: false,
            alcohol_potable: false,
            quality: 0,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
        }
    }
}

fn core_slot(value: sats::Slot) -> Slot {
    match value {
        sats::Slot::None => Slot::None,
        sats::Slot::LeftHolding => Slot::LeftHolding,
        sats::Slot::RightHolding => Slot::RightHolding,
        sats::Slot::LeftArm => Slot::LeftArm,
        sats::Slot::RightArm => Slot::RightArm,
        sats::Slot::LeftLeg => Slot::LeftLeg,
        sats::Slot::RightLeg => Slot::RightLeg,
        sats::Slot::Chest => Slot::Chest,
        sats::Slot::Stomach => Slot::Stomach,
        sats::Slot::Head => Slot::Head,
        sats::Slot::AnyHolding => Slot::AnyHolding,
        sats::Slot::AnyArm => Slot::AnyArm,
        sats::Slot::AnyLeg => Slot::AnyLeg,
    }
}

pub fn core_equipment_location(value: sats::EquipmentLocation) -> EquipmentLocation {
    match value {
        sats::EquipmentLocation::Head => EquipmentLocation::Head,
        sats::EquipmentLocation::Face => EquipmentLocation::Face,
        sats::EquipmentLocation::Neck => EquipmentLocation::Neck,
        sats::EquipmentLocation::Chest => EquipmentLocation::Chest,
        sats::EquipmentLocation::Stomach => EquipmentLocation::Stomach,
        sats::EquipmentLocation::Back => EquipmentLocation::Back,
        sats::EquipmentLocation::LeftShoulder => EquipmentLocation::LeftShoulder,
        sats::EquipmentLocation::RightShoulder => EquipmentLocation::RightShoulder,
        sats::EquipmentLocation::LeftArm => EquipmentLocation::LeftArm,
        sats::EquipmentLocation::RightArm => EquipmentLocation::RightArm,
        sats::EquipmentLocation::LeftHand => EquipmentLocation::LeftHand,
        sats::EquipmentLocation::RightHand => EquipmentLocation::RightHand,
        sats::EquipmentLocation::LeftLeg => EquipmentLocation::LeftLeg,
        sats::EquipmentLocation::RightLeg => EquipmentLocation::RightLeg,
        sats::EquipmentLocation::LeftFoot => EquipmentLocation::LeftFoot,
        sats::EquipmentLocation::RightFoot => EquipmentLocation::RightFoot,
        sats::EquipmentLocation::LeftBelt => EquipmentLocation::LeftBelt,
        sats::EquipmentLocation::RightBelt => EquipmentLocation::RightBelt,
        sats::EquipmentLocation::FrontBelt => EquipmentLocation::FrontBelt,
        sats::EquipmentLocation::BackBelt => EquipmentLocation::BackBelt,
        sats::EquipmentLocation::LeftPocket => EquipmentLocation::LeftPocket,
        sats::EquipmentLocation::RightPocket => EquipmentLocation::RightPocket,
        sats::EquipmentLocation::BackLeftPocket => EquipmentLocation::BackLeftPocket,
        sats::EquipmentLocation::BackRightPocket => EquipmentLocation::BackRightPocket,
    }
}

pub fn core_body_region(value: sats::BodyRegion) -> BodyRegion {
    match value {
        sats::BodyRegion::LeftArm => BodyRegion::LeftArm,
        sats::BodyRegion::RightArm => BodyRegion::RightArm,
        sats::BodyRegion::LeftLeg => BodyRegion::LeftLeg,
        sats::BodyRegion::RightLeg => BodyRegion::RightLeg,
        sats::BodyRegion::Chest => BodyRegion::Chest,
        sats::BodyRegion::Abdomen => BodyRegion::Abdomen,
        sats::BodyRegion::Head => BodyRegion::Head,
    }
}

pub fn core_intervention_route(value: sats::InterventionRoute) -> InterventionRoute {
    match value {
        sats::InterventionRoute::Oral => InterventionRoute::Oral,
        sats::InterventionRoute::Topical => InterventionRoute::Topical,
        sats::InterventionRoute::Inhaled => InterventionRoute::Inhaled,
        sats::InterventionRoute::Injected => InterventionRoute::Injected,
    }
}

pub fn core_personality_axis(value: sats::PersonalityAxis) -> PersonalityAxis {
    match value {
        sats::PersonalityAxis::Nerve => PersonalityAxis::Nerve,
        sats::PersonalityAxis::Drive => PersonalityAxis::Drive,
        sats::PersonalityAxis::Outlook => PersonalityAxis::Outlook,
        sats::PersonalityAxis::Sociability => PersonalityAxis::Sociability,
        sats::PersonalityAxis::Conscience => PersonalityAxis::Conscience,
        sats::PersonalityAxis::SelfRegard => PersonalityAxis::SelfRegard,
        sats::PersonalityAxis::Conviction => PersonalityAxis::Conviction,
        sats::PersonalityAxis::Hygiene => PersonalityAxis::Hygiene,
        sats::PersonalityAxis::Temperance => PersonalityAxis::Temperance,
        sats::PersonalityAxis::Mirth => PersonalityAxis::Mirth,
        sats::PersonalityAxis::Courtship => PersonalityAxis::Courtship,
        sats::PersonalityAxis::Transparency => PersonalityAxis::Transparency,
        sats::PersonalityAxis::SelfKnowledge => PersonalityAxis::SelfKnowledge,
        sats::PersonalityAxis::Inclination => PersonalityAxis::Inclination,
        sats::PersonalityAxis::Presentation => PersonalityAxis::Presentation,
    }
}

pub fn core_incapacitation_status(
    value: sats::IncapacitationStatus,
) -> adventuresim_core::morale::IncapacitationStatus {
    match value {
        sats::IncapacitationStatus::Ready => adventuresim_core::morale::IncapacitationStatus::Ready,
        sats::IncapacitationStatus::Staggered => {
            adventuresim_core::morale::IncapacitationStatus::Staggered
        }
        sats::IncapacitationStatus::Incapacitated => {
            adventuresim_core::morale::IncapacitationStatus::Incapacitated
        }
    }
}

pub fn core_investigation_action_availability(
    value: &sats::InvestigationActionAvailability,
) -> InvestigationActionAvailability {
    match value {
        sats::InvestigationActionAvailability::Available => {
            InvestigationActionAvailability::Available
        }
        sats::InvestigationActionAvailability::Unavailable(details) => {
            let sats::InvestigationActionUnavailableFields {
                reason,
                can_travel_to_required_site,
                wait_minutes,
            } = details.clone();
            InvestigationActionAvailability::Unavailable {
                reason: match reason {
                    sats::InvestigationActionUnavailableReason::PartyNotReady => {
                        InvestigationActionUnavailableReason::PartyNotReady
                    }
                    sats::InvestigationActionUnavailableReason::TravelRequired => {
                        InvestigationActionUnavailableReason::TravelRequired
                    }
                    sats::InvestigationActionUnavailableReason::NightWindow => {
                        InvestigationActionUnavailableReason::NightWindow
                    }
                    sats::InvestigationActionUnavailableReason::TargetChanged => {
                        InvestigationActionUnavailableReason::TargetChanged
                    }
                    sats::InvestigationActionUnavailableReason::ContactScheduleWindow => {
                        InvestigationActionUnavailableReason::ContactScheduleWindow
                    }
                    sats::InvestigationActionUnavailableReason::ContactNotPresent => {
                        InvestigationActionUnavailableReason::ContactNotPresent
                    }
                    sats::InvestigationActionUnavailableReason::CharacterUnavailable => {
                        InvestigationActionUnavailableReason::CharacterUnavailable
                    }
                    sats::InvestigationActionUnavailableReason::PartyRequired => {
                        InvestigationActionUnavailableReason::PartyRequired
                    }
                },
                can_travel_to_required_site,
                wait_minutes,
            }
        }
    }
}

pub fn core_equipment_channel(value: sats::EquipmentChannel) -> EquipmentChannel {
    match value {
        sats::EquipmentChannel::Held => EquipmentChannel::Held,
        sats::EquipmentChannel::BaseClothing => EquipmentChannel::BaseClothing,
        sats::EquipmentChannel::Padding => EquipmentChannel::Padding,
        sats::EquipmentChannel::FlexibleArmor => EquipmentChannel::FlexibleArmor,
        sats::EquipmentChannel::RigidArmor => EquipmentChannel::RigidArmor,
        sats::EquipmentChannel::Outerwear => EquipmentChannel::Outerwear,
        sats::EquipmentChannel::Accessory => EquipmentChannel::Accessory,
        sats::EquipmentChannel::Mount => EquipmentChannel::Mount,
        sats::EquipmentChannel::Containment => EquipmentChannel::Containment,
    }
}

fn core_equipment_body_part(value: sats::EquipmentBodyPart) -> EquipmentBodyPart {
    match value {
        sats::EquipmentBodyPart::LeftArm => EquipmentBodyPart::LeftArm,
        sats::EquipmentBodyPart::RightArm => EquipmentBodyPart::RightArm,
        sats::EquipmentBodyPart::LeftLeg => EquipmentBodyPart::LeftLeg,
        sats::EquipmentBodyPart::RightLeg => EquipmentBodyPart::RightLeg,
        sats::EquipmentBodyPart::Chest => EquipmentBodyPart::Chest,
        sats::EquipmentBodyPart::Stomach => EquipmentBodyPart::Stomach,
        sats::EquipmentBodyPart::Head => EquipmentBodyPart::Head,
    }
}

fn core_melee_style(value: sats::MeleeAttackStyle) -> MeleeAttackStyle {
    match value {
        sats::MeleeAttackStyle::Swing => MeleeAttackStyle::Swing,
        sats::MeleeAttackStyle::Stab => MeleeAttackStyle::Stab,
    }
}

fn catalog_item_kind(value: sats::PersistedItemKind) -> CatalogItemKind {
    match value {
        sats::PersistedItemKind::Simple => CatalogItemKind::Simple,
        sats::PersistedItemKind::Weapon => CatalogItemKind::Weapon,
        sats::PersistedItemKind::Armor => CatalogItemKind::Armor,
        sats::PersistedItemKind::Shield => CatalogItemKind::Shield,
        sats::PersistedItemKind::Clothing => CatalogItemKind::Clothing,
        sats::PersistedItemKind::Container => CatalogItemKind::Container,
        sats::PersistedItemKind::Currency => CatalogItemKind::Currency,
        sats::PersistedItemKind::Ingredient => CatalogItemKind::Ingredient,
        sats::PersistedItemKind::Medication => CatalogItemKind::Medication,
        sats::PersistedItemKind::Food => CatalogItemKind::Food,
    }
}

fn core_weapon_skills(value: sats::WeaponSkillDistribution) -> WeaponSkillDistribution {
    let sats::WeaponSkillDistribution {
        polearm,
        axe,
        bludgeon,
        sword,
        knife,
        bow,
        crossbow,
        firearm,
        throw,
    } = value;
    WeaponSkillDistribution {
        polearm,
        axe,
        bludgeon,
        sword,
        knife,
        bow,
        crossbow,
        firearm,
        throw,
    }
}

pub trait JourneyEndpointExt {
    fn settlement_id(&self) -> Option<&str>;
    fn case_site_id(&self) -> Option<&str>;
    fn name(&self) -> &str;
}

impl JourneyEndpointExt for JourneyEndpoint {
    fn settlement_id(&self) -> Option<&str> {
        match self {
            Self::Settlement(endpoint) => Some(&endpoint.id),
            _ => None,
        }
    }

    fn case_site_id(&self) -> Option<&str> {
        match self {
            Self::CaseSite(endpoint) => Some(&endpoint.id.value),
            _ => None,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Settlement(endpoint) => &endpoint.name,
            Self::CaseSite(endpoint) => &endpoint.name,
            Self::Camp(_) => "Camp",
        }
    }
}

pub trait ItemConditionExt {
    fn bins(&self) -> [f32; 5];
    fn total(&self) -> f32;
    fn repairable(&self, skill: u8) -> f32;
    fn residual(&self, skill: u8) -> f32;
}

pub trait ReligionHoursExt {
    fn direct(&self, religion: adventuresim_world_schema::OfficialReligion) -> f32;
    fn effective(&self, religion: adventuresim_world_schema::OfficialReligion) -> f32;
    fn total_direct(&self) -> f32;
}

#[cfg(test)]
pub(crate) fn generated_character_skills_fixture() -> sats::CharacterSkills {
    sats::CharacterSkills {
        character_id: 0,
        polearm_hours: 0.0,
        axe_hours: 0.0,
        bludgeon_hours: 0.0,
        sword_hours: 0.0,
        knife_hours: 0.0,
        dodge_hours: 0.0,
        block_hours: 0.0,
        bow_hours: 0.0,
        crossbow_hours: 0.0,
        firearm_hours: 0.0,
        throw_hours: 0.0,
        will_hours: 0.0,
        insight_hours: 0.0,
        charm_hours: 0.0,
        command_hours: 0.0,
        deception_hours: 0.0,
        physiology_hours: 0.0,
        cooking_hours: 0.0,
        herbalism_hours: 0.0,
        religion_hours: sats::ReligionHours {
            roman_catholic: 0.0,
            lutheran: 0.0,
            reformed: 0.0,
            anglican: 0.0,
            eastern_orthodox: 0.0,
            islamic: 0.0,
            judaism: 0.0,
        },
        bestiary_hours: sats::BestiaryHours {
            beast: 0.0,
            undead: 0.0,
            human: 0.0,
            werekin: 0.0,
            elf: 0.0,
            dwarf: 0.0,
            fey: 0.0,
            spirit: 0.0,
            greenskin: 0.0,
            insectoid: 0.0,
            draconid: 0.0,
            construct: 0.0,
            wildmen: 0.0,
        },
        oral_languages: sats::OralLanguageHours {
            east_central: 0.0,
            west_central: 0.0,
            low: 0.0,
            yiddish: 0.0,
            latin: 0.0,
            romani: 0.0,
            elven: 0.0,
            dwarfish: 0.0,
        },
        written_languages: sats::WrittenLanguageHours {
            german: 0.0,
            low: 0.0,
            latin: 0.0,
            hebrew: 0.0,
            yiddish: 0.0,
            elven: 0.0,
            dwarfish: 0.0,
        },
        stealth_hours: 0.0,
        balance_hours: 0.0,
        terrain_plains_hours: 0.0,
        terrain_forest_hours: 0.0,
        terrain_hills_hours: 0.0,
        terrain_wetlands_hours: 0.0,
        terrain_urban_hours: 0.0,
        terrain_snow_hours: 0.0,
        surgery_hours: 0.0,
        tailoring_hours: 0.0,
        smithing_hours: 0.0,
    }
}

impl ReligionHoursExt for sats::ReligionHours {
    fn direct(&self, religion: adventuresim_world_schema::OfficialReligion) -> f32 {
        core_religion_hours(self).direct(religion)
    }

    fn effective(&self, religion: adventuresim_world_schema::OfficialReligion) -> f32 {
        core_religion_hours(self).effective(religion)
    }

    fn total_direct(&self) -> f32 {
        core_religion_hours(self).total_direct()
    }
}

pub trait OralLanguageHoursExt {
    fn direct(&self, language: adventuresim_world_schema::OralLanguage) -> f32;
    fn effective(&self, language: adventuresim_world_schema::OralLanguage) -> f32;
}

impl OralLanguageHoursExt for sats::OralLanguageHours {
    fn direct(&self, language: adventuresim_world_schema::OralLanguage) -> f32 {
        core_oral_language_hours(self).direct(language)
    }

    fn effective(&self, language: adventuresim_world_schema::OralLanguage) -> f32 {
        core_oral_language_hours(self).effective(language)
    }
}

pub trait WrittenLanguageHoursExt {
    fn direct(&self, language: adventuresim_world_schema::WrittenLanguage) -> f32;
    fn effective(&self, language: adventuresim_world_schema::WrittenLanguage) -> f32;
}

impl WrittenLanguageHoursExt for sats::WrittenLanguageHours {
    fn direct(&self, language: adventuresim_world_schema::WrittenLanguage) -> f32 {
        core_written_language_hours(self).direct(language)
    }

    fn effective(&self, language: adventuresim_world_schema::WrittenLanguage) -> f32 {
        core_written_language_hours(self).effective(language)
    }
}

pub trait BestiaryHoursExt {
    fn direct(&self, category: adventuresim_world_schema::BestiaryCategory) -> f32;
    fn effective(&self, category: adventuresim_world_schema::BestiaryCategory) -> f32;
    fn aggregate_effective(&self) -> f32;
    fn total_direct(&self) -> f32;
}

impl BestiaryHoursExt for sats::BestiaryHours {
    fn direct(&self, category: adventuresim_world_schema::BestiaryCategory) -> f32 {
        core_bestiary_hours(self).direct(category)
    }

    fn effective(&self, category: adventuresim_world_schema::BestiaryCategory) -> f32 {
        core_bestiary_hours(self).effective(category)
    }

    fn aggregate_effective(&self) -> f32 {
        core_bestiary_hours(self).aggregate_effective()
    }

    fn total_direct(&self) -> f32 {
        core_bestiary_hours(self).total_direct()
    }
}

fn core_religion_hours(value: &sats::ReligionHours) -> adventuresim_world_schema::ReligionHours {
    adventuresim_world_schema::ReligionHours {
        roman_catholic: value.roman_catholic,
        lutheran: value.lutheran,
        reformed: value.reformed,
        anglican: value.anglican,
        eastern_orthodox: value.eastern_orthodox,
        islamic: value.islamic,
        judaism: value.judaism,
    }
}

pub fn religion_hours_from_core(
    value: &adventuresim_world_schema::ReligionHours,
) -> sats::ReligionHours {
    sats::ReligionHours {
        roman_catholic: value.roman_catholic,
        lutheran: value.lutheran,
        reformed: value.reformed,
        anglican: value.anglican,
        eastern_orthodox: value.eastern_orthodox,
        islamic: value.islamic,
        judaism: value.judaism,
    }
}

pub fn core_oral_language_hours(
    value: &sats::OralLanguageHours,
) -> adventuresim_world_schema::OralLanguageHours {
    adventuresim_world_schema::OralLanguageHours {
        east_central: value.east_central,
        west_central: value.west_central,
        low: value.low,
        yiddish: value.yiddish,
        latin: value.latin,
        romani: value.romani,
        elven: value.elven,
        dwarfish: value.dwarfish,
    }
}

pub fn core_morale_source_kind(
    value: sats::MoraleSourceKind,
) -> adventuresim_core::morale::MoraleSourceKind {
    use adventuresim_core::morale::MoraleSourceKind as Core;
    match value {
        sats::MoraleSourceKind::Injury => Core::Injury,
        sats::MoraleSourceKind::Cleanliness => Core::Cleanliness,
        sats::MoraleSourceKind::Religion => Core::Religion,
        sats::MoraleSourceKind::ReligiousDiscord => Core::ReligiousDiscord,
        sats::MoraleSourceKind::Prayer => Core::Prayer,
        sats::MoraleSourceKind::Meditation => Core::Meditation,
        sats::MoraleSourceKind::Power => Core::Power,
        sats::MoraleSourceKind::Ally => Core::Ally,
        sats::MoraleSourceKind::CorpseHandling => Core::CorpseHandling,
        sats::MoraleSourceKind::SocialInteraction => Core::SocialInteraction,
        sats::MoraleSourceKind::WitnessCharm => Core::WitnessCharm,
        sats::MoraleSourceKind::WitnessCommand => Core::WitnessCommand,
        sats::MoraleSourceKind::WitnessBluff => Core::WitnessBluff,
        sats::MoraleSourceKind::Victory => Core::Victory,
        sats::MoraleSourceKind::Defeat => Core::Defeat,
        sats::MoraleSourceKind::MasteryEnjoyment => Core::MasteryEnjoyment,
        sats::MoraleSourceKind::ReligiousObservanceNeglected => Core::ReligiousObservanceNeglected,
        sats::MoraleSourceKind::HolyDayObserved => Core::HolyDayObserved,
        sats::MoraleSourceKind::TravelPrayerNeglected => Core::TravelPrayerNeglected,
        sats::MoraleSourceKind::SpouseLeisure => Core::SpouseLeisure,
        sats::MoraleSourceKind::Carousing => Core::Carousing,
        sats::MoraleSourceKind::AlcoholSatisfied => Core::AlcoholSatisfied,
        sats::MoraleSourceKind::AlcoholUnsatisfied => Core::AlcoholUnsatisfied,
        sats::MoraleSourceKind::ResidenceLeisure => Core::ResidenceLeisure,
        sats::MoraleSourceKind::Leisure => Core::Leisure,
    }
}

pub fn empty_oral_language_hours() -> sats::OralLanguageHours {
    sats::OralLanguageHours {
        east_central: 0.0,
        west_central: 0.0,
        low: 0.0,
        yiddish: 0.0,
        latin: 0.0,
        romani: 0.0,
        elven: 0.0,
        dwarfish: 0.0,
    }
}

fn core_written_language_hours(
    value: &sats::WrittenLanguageHours,
) -> adventuresim_world_schema::WrittenLanguageHours {
    adventuresim_world_schema::WrittenLanguageHours {
        german: value.german,
        low: value.low,
        latin: value.latin,
        hebrew: value.hebrew,
        yiddish: value.yiddish,
        elven: value.elven,
        dwarfish: value.dwarfish,
    }
}

pub fn empty_written_language_hours() -> sats::WrittenLanguageHours {
    sats::WrittenLanguageHours {
        german: 0.0,
        low: 0.0,
        latin: 0.0,
        hebrew: 0.0,
        yiddish: 0.0,
        elven: 0.0,
        dwarfish: 0.0,
    }
}

fn core_bestiary_hours(value: &sats::BestiaryHours) -> adventuresim_world_schema::BestiaryHours {
    adventuresim_world_schema::BestiaryHours {
        beast: value.beast,
        undead: value.undead,
        human: value.human,
        werekin: value.werekin,
        elf: value.elf,
        dwarf: value.dwarf,
        fey: value.fey,
        spirit: value.spirit,
        greenskin: value.greenskin,
        insectoid: value.insectoid,
        draconid: value.draconid,
        construct: value.construct,
        wildmen: value.wildmen,
    }
}

pub fn bestiary_hours_from_core(
    value: &adventuresim_world_schema::BestiaryHours,
) -> sats::BestiaryHours {
    sats::BestiaryHours {
        beast: value.beast,
        undead: value.undead,
        human: value.human,
        werekin: value.werekin,
        elf: value.elf,
        dwarf: value.dwarf,
        fey: value.fey,
        spirit: value.spirit,
        greenskin: value.greenskin,
        insectoid: value.insectoid,
        draconid: value.draconid,
        construct: value.construct,
        wildmen: value.wildmen,
    }
}

impl ItemConditionExt for ItemCondition {
    fn bins(&self) -> [f32; 5] {
        [
            self.tier_1,
            self.tier_2,
            self.tier_3,
            self.tier_4,
            self.tier_5,
        ]
    }
    fn total(&self) -> f32 {
        self.bins().iter().sum::<f32>().clamp(0.0, 1.0)
    }
    fn repairable(&self, skill: u8) -> f32 {
        self.bins().iter().take(skill.min(5) as usize).sum()
    }
    fn residual(&self, skill: u8) -> f32 {
        self.bins().iter().skip(skill.min(5) as usize).sum()
    }
}

pub fn core_personality(row: &sats::CharacterPersonality) -> Personality {
    let sats::CharacterPersonality {
        character_id: _,
        projection_character_id: _,
        nerve,
        drive,
        outlook,
        sociability,
        conscience,
        self_regard,
        conviction,
        hygiene,
        temperance,
        mirth,
        courtship,
        transparency,
        self_knowledge,
        sex,
        presentation,
        inclination,
    } = row.clone();
    Personality {
        nerve: match nerve {
            sats::Nerve::Neutral => Nerve::Neutral,
            sats::Nerve::Brave => Nerve::Brave,
            sats::Nerve::Fearful => Nerve::Fearful,
        },
        drive: match drive {
            sats::Drive::Neutral => Drive::Neutral,
            sats::Drive::Ambitious => Drive::Ambitious,
            sats::Drive::Content => Drive::Content,
        },
        outlook: match outlook {
            sats::Outlook::Neutral => Outlook::Neutral,
            sats::Outlook::Sanguine => Outlook::Sanguine,
            sats::Outlook::Brooding => Outlook::Brooding,
        },
        sociability: match sociability {
            sats::Sociability::Neutral => Sociability::Neutral,
            sats::Sociability::Gregarious => Sociability::Gregarious,
            sats::Sociability::Solitary => Sociability::Solitary,
        },
        conscience: match conscience {
            sats::Conscience::Neutral => Conscience::Neutral,
            sats::Conscience::Compassionate => Conscience::Compassionate,
            sats::Conscience::Callous => Conscience::Callous,
            sats::Conscience::Cruel => Conscience::Cruel,
        },
        self_regard: match self_regard {
            sats::SelfRegard::Neutral => SelfRegard::Neutral,
            sats::SelfRegard::Proud => SelfRegard::Proud,
            sats::SelfRegard::Humble => SelfRegard::Humble,
        },
        conviction: match conviction {
            sats::Conviction::Neutral => Conviction::Neutral,
            sats::Conviction::Zealous => Conviction::Zealous,
            sats::Conviction::Irreverent => Conviction::Irreverent,
        },
        hygiene: match hygiene {
            sats::Hygiene::Neutral => Hygiene::Neutral,
            sats::Hygiene::Slovenly => Hygiene::Slovenly,
            sats::Hygiene::Cleanly => Hygiene::Cleanly,
        },
        temperance: match temperance {
            sats::Temperance::Neutral => Temperance::Neutral,
            sats::Temperance::Temperate => Temperance::Temperate,
            sats::Temperance::Drunkard => Temperance::Drunkard,
        },
        mirth: match mirth {
            sats::Mirth::Neutral => Mirth::Neutral,
            sats::Mirth::Merry => Mirth::Merry,
            sats::Mirth::Grave => Mirth::Grave,
        },
        courtship: match courtship {
            sats::Courtship::Neutral => Courtship::Neutral,
            sats::Courtship::Amorous => Courtship::Amorous,
            sats::Courtship::Proper => Courtship::Proper,
        },
        transparency: match transparency {
            sats::Transparency::Neutral => Transparency::Neutral,
            sats::Transparency::Open => Transparency::Open,
            sats::Transparency::Guarded => Transparency::Guarded,
        },
        self_knowledge: match self_knowledge {
            sats::SelfKnowledge::Neutral => SelfKnowledge::Neutral,
            sats::SelfKnowledge::Introspective => SelfKnowledge::Introspective,
            sats::SelfKnowledge::SelfDeceiving => SelfKnowledge::SelfDeceiving,
        },
        inclination: match inclination {
            sats::Inclination::Men => Inclination::Men,
            sats::Inclination::Either => Inclination::Either,
            sats::Inclination::Women => Inclination::Women,
            sats::Inclination::Neither => Inclination::Neither,
        },
        presentation: match presentation {
            sats::Presentation::Man => Presentation::Man,
            sats::Presentation::Ambiguous => Presentation::Ambiguous,
            sats::Presentation::Woman => Presentation::Woman,
        },
        sex: match sex {
            sats::Sex::Female => Sex::Female,
            sats::Sex::Male => Sex::Male,
        },
    }
}

pub fn role_requirements(row: &sats::RoleRequirements) -> RoleRequirements {
    let sats::RoleRequirements {
        melee,
        ranged,
        weapon_precision,
        heavy,
        quarter_armor,
        half_armor,
        three_quarter_armor,
        full_armor,
        athletics,
        endurance,
        physiology,
        surgery,
        command,
        religion,
    } = row.clone();
    RoleRequirements {
        melee,
        ranged,
        weapon_precision,
        heavy,
        quarter_armor,
        half_armor,
        three_quarter_armor,
        full_armor,
        athletics,
        endurance,
        physiology,
        surgery,
        command,
        religion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated_role_requirements() -> sats::RoleRequirements {
        sats::RoleRequirements {
            melee: true,
            ranged: false,
            weapon_precision: 0.75,
            heavy: true,
            quarter_armor: true,
            half_armor: false,
            three_quarter_armor: false,
            full_armor: false,
            athletics: 2,
            endurance: 3,
            physiology: 4,
            surgery: 5,
            command: 1,
            religion: 2,
        }
    }

    fn generated_settlement() -> sats::Settlement {
        sats::Settlement {
            id: "lubeck".into(),
            name: "Lubeck".into(),
            coord_x: 10.6866,
            coord_y: 53.8655,
            population_level: 5,
            population_estimate: 22_000,
            category: sats::SettlementCategory::City,
            elevation: sats::ElevationMeters { meters: 0 },
            land_use: sats::LandUseProfile {
                cropland: sats::LandUseFraction { basis_points: 0 },
                grazing: sats::LandUseFraction { basis_points: 0 },
                built_up: sats::LandUseFraction { basis_points: 0 },
                natural: sats::LandUseFraction {
                    basis_points: 10_000,
                },
            },
            forest_cover: sats::ForestCover::Open,
            potential_vegetation: sats::PotentialVegetation::Categorical(
                sats::PotentialVegetationClass::Grassland,
            ),
            historical_vegetation: sats::HistoricalVegetation::Fallback(
                sats::FallbackHistoricalVegetation {
                    cover: sats::FallbackHistoricalVegetationCover::Grassland,
                    method: sats::FallbackHistoricalVegetationMethod::PotentialEnvelopeV4,
                },
            ),
            tree_species: sats::TreeSpeciesProfile::Inferred(sats::InferredTreeSpeciesProfile {
                species: Vec::new(),
            }),
            soil: sats::SoilProfile {
                wrb_group: sats::WrbReferenceGroup::Regosol,
                parent_material: sats::SurfaceLithology::Unconsolidated(
                    sats::UnconsolidatedDeposit::Sand,
                ),
                properties: sats::SoilProperties {
                    substrate: sats::SoilSubstrate::RockOutcrop(sats::RockOutcropSoil {
                        stones: sats::StoneContentPercent { percent: 100 },
                    }),
                    water_regime: sats::SoilWaterRegime::UsuallyDry,
                    agricultural_limitation: sats::AgriculturalLimitation::ShallowRock,
                },
                acidity: sats::SoilAcidity::Neutral,
                cation_exchange_capacity: sats::CationExchangeCapacity::Low,
                fertility: sats::SoilFertility::Low,
                confidence: sats::SoilBasisPoints { value: 1_000 },
                evidence: sats::SoilEvidence::DeterministicInference,
            },
            geology: sats::SurfaceGeology::Inferred(sats::InferredGeologicSetting {
                lithology: sats::SurfaceLithology::Unconsolidated(
                    sats::UnconsolidatedDeposit::Sand,
                ),
                age: sats::GeologicEra::Quaternary,
            }),
            religious_status: sats::SettlementReligiousStatus::Established(
                sats::OfficialReligion::RomanCatholic,
            ),
            languages: sats::SettlementLanguageProfile {
                east_central_bp: 10_000,
                west_central_bp: 0,
                low_bp: 0,
                yiddish_incidence_bp: 125,
            },
            drought: sats::DroughtProfile::Inferred(sats::DroughtHistory {
                current_summer: sats::PalmerDroughtSeverityIndex { milli_units: 0 },
                twenty_year_mean: sats::PalmerDroughtSeverityIndex { milli_units: 0 },
                drought_summers: 0,
                wet_summers: 0,
            }),
            hydrology: sats::SettlementHydrology {
                flowing: None,
                inland: None,
                marine: None,
            },
            industries: sats::InferredIndustryProfile {
                outputs: vec![sats::IndustryEvidence::Fallback(
                    sats::FallbackIndustry::WoodlandFuelwood,
                )],
            },
            economy: sats::SettlementEconomyProfile {
                rules_version: 10,
                prosperity_score: 0,
                prosperity_tier: sats::ProsperityTier::Subsistence,
                services: vec![sats::SettlementService::Inn],
                specializations: Vec::new(),
                stock: vec![sats::SettlementStock {
                    category: sats::StockCategory::GeneralGoods,
                    abundance: 1,
                    provenance: sats::ProfileFactProvenance::DeterministicGapFill,
                }],
            },
            scene_key: "lubeck-market".into(),
            religion_id: "roman_catholic".into(),
            currency_id: "lubeck_penny".into(),
            source_node_id: Some(52),
            sources: "fixture evidence".into(),
        }
    }

    fn generated_travel_edge() -> sats::TravelEdge {
        sats::TravelEdge {
            id: 41,
            from_node_id: 52,
            to_node_id: 53,
            route: sats::TravelRoute::Land(sats::LandRoute {
                bridge: None,
                water_crossings: Vec::new(),
            }),
            provenance: sats::TravelEdgeProvenance::DocumentedViabundus,
            toll_at: None,
            length_m: 1_250,
            slope_multiplier: 1.25,
            terrain: sats::RouteTerrain {
                elevation_profile: sats::RouteElevationProfile {
                    samples: vec![
                        sats::RouteElevationSample {
                            progress: sats::EdgeProgressPermille { permille: 0 },
                            elevation: sats::ElevationMeters { meters: 0 },
                        },
                        sats::RouteElevationSample {
                            progress: sats::EdgeProgressPermille { permille: 1_000 },
                            elevation: sats::ElevationMeters { meters: 0 },
                        },
                    ],
                },
                ascent: sats::RouteVerticalMeters { meters: 0 },
                descent: sats::RouteVerticalMeters { meters: 0 },
                max_uphill_grade: sats::RouteSignedGradePermille { permille: 0 },
                max_downhill_grade: sats::RouteSignedGradePermille { permille: 0 },
                mean_slope: sats::RouteSlopePermille { permille: 0 },
                max_slope: sats::RouteSlopePermille { permille: 0 },
                dominant_aspect: sats::DominantAspect::Flat,
                roughness: sats::RouteRoughnessMeters { meters: 0 },
                relief: sats::RouteReliefMeters { meters: 0 },
                landforms: Vec::new(),
                class: sats::RouteTerrainClass::Flat,
                water_adjacencies: Vec::new(),
                seasonal_risks: Vec::new(),
                encounter_tags: vec![sats::RouteEncounterTag::Flat],
            },
            certainty: 90,
            section: "52:53".into(),
            sources: "fixture evidence".into(),
        }
    }

    fn generated_item() -> sats::Item {
        sats::Item {
            id: "arming_sword".into(),
            weight: 1.25,
            exterior_volume_ml: 900,
            slot: sats::Slot::AnyHolding,
            kind: sats::PersistedItemKind::Weapon,
            equipment_placements: vec![sats::PersistedEquipmentPlacement {
                id: "right_hand".into(),
                occupancy: vec![sats::OccupancyRequirement {
                    location: sats::EquipmentLocation::RightHand,
                    channel: sats::EquipmentChannel::Held,
                    order: 2,
                }],
                parents: vec![sats::ParentRequirement {
                    channel: sats::EquipmentChannel::Mount,
                    order: 3,
                }],
                protection: vec![sats::EquipmentBodyPart::RightArm],
            }],
            attachment_tags: vec!["blade".into()],
            attachment_points: vec![sats::PersistedEquipmentAttachmentPoint {
                id: "pommel".into(),
                channel: sats::EquipmentChannel::Accessory,
                capacity: 2,
                order: 4,
                accepts_tags: vec!["charm".into()],
            }],
            repairable: true,
            accuracy: 0.1,
            swing_precision: 0.2,
            stab_precision: 0.3,
            preferred_melee_style: sats::MeleeAttackStyle::Stab,
            reach: 1.1,
            block: 0.4,
            coverage: 0.5,
            penetration: 0.6,
            resistance: 0.7,
            padding: 0.8,
            flexibility: 0.9,
            range_of_motion: 1.0,
            precise: true,
            moment_of_inertia_kg_m_2: 1.2,
            balance: 1.3,
            melee: true,
            ranged: false,
            weapon_skills: sats::WeaponSkillDistribution {
                polearm: 0.0,
                axe: 0.0,
                bludgeon: 0.0,
                sword: 1.0,
                knife: 0.1,
                bow: 0.0,
                crossbow: 0.0,
                firearm: 0.0,
                throw: 0.2,
            },
            blunt: false,
            slash: true,
            pierce: true,
            base_value: Some(25),
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            container_capacity_ml: 0,
            alcohol_serving_ml: 0,
            alcohol_abv_basis_points: 0,
            alcohol_net_hydration_ml: 0,
            alcohol_disinfectant_effectiveness: 0,
            alcohol_disinfectant_focused: false,
            alcohol_potable: false,
            quality: 3,
            durability_yield: 0.11,
            durability_fracture: 0.22,
            durability_wear: 0.33,
            durability_failure_share: 0.44,
            edge_sensitivity: 0.55,
            handling_sensitivity: 0.66,
        }
    }

    #[test]
    fn generated_rows_map_to_views_with_explicit_enrichment_and_gateway_fields() {
        let character = CharacterView::from(sats::Character {
            id: 7,
            scan_id: 700,
            name: "Ada".into(),
            xp: 12,
            level: 3,
            current_settlement_id: Some("lubeck".into()),
            party_id: Some("party:7".into()),
            server: sats::spacetimedb_sdk::Identity::ZERO,
            in_server: true,
            temporary: false,
            age_years: 24,
            alive: true,
            party_treatment_decision: sats::ContextualDecisionState::Allowed,
        });
        assert_eq!((character.id, character.name.as_str()), (7, "Ada"));
        assert_eq!(character.current_case_site_id, None);
        assert_eq!(character.social_notification_count, 0);
        assert!(!character.automatic_social_chat_enabled);

        let settlement = SettlementView::try_from(generated_settlement()).unwrap();
        assert_eq!(
            (settlement.id.as_str(), settlement.name.as_str()),
            ("lubeck", "Lubeck")
        );
        assert_eq!(
            (settlement.longitude, settlement.latitude),
            (10.6866, 53.8655)
        );
        assert_eq!(settlement.languages.east_central_bp, 10_000);
        assert_eq!(settlement.industries.outputs().len(), 1);
        assert_eq!(settlement.economy.rules_version, 10);
        assert_eq!(settlement.source_node_id, Some(52));

        let travel_edge = TravelEdgeView::try_from(generated_travel_edge()).unwrap();
        assert_eq!(
            (
                travel_edge.id,
                travel_edge.from_node_id,
                travel_edge.to_node_id
            ),
            (41, 52, 53)
        );
        assert!(matches!(
            travel_edge.route,
            adventuresim_world_schema::TravelRoute::Land(_)
        ));
        assert_eq!(
            travel_edge.terrain,
            adventuresim_world_schema::RouteTerrain::stage_placeholder()
        );
        assert_eq!((travel_edge.length_m, travel_edge.certainty), (1_250, 90));

        let case_battle = CaseBattleView::try_from(sats::BackendCaseBattle {
            gateway_bucket: 6,
            owner_character_id: 7,
            public_case_id: "case:1".into(),
            party_id: "party:7".into(),
            battle_id: "battle:1".into(),
            mission_id: "mission:1".into(),
            case_site_id: sats::CaseSiteId {
                value: "site:1".into(),
            },
        })
        .unwrap();
        assert_eq!(case_battle.gateway_bucket, 6);
        assert_eq!(case_battle.case_site_id.as_str(), "site:1");

        let party = PartyView::try_from(sats::Party {
            id: "party:7".into(),
            gateway_bucket: 5,
            name: "Company".into(),
            leader_id: 7,
            current_settlement_id: None,
            current_case_site_id: Some(sats::CaseSiteId {
                value: "site:1".into(),
            }),
            active_contract_id: Some("contract:1".into()),
            is_solo: false,
            camp_fatigue_percent: 25,
            walking_minutes_per_day: 480,
            travel_at_night: true,
            journey_start_minute_of_day: 360,
            wilderness_canonical_anchor_minute: Some(1_000),
            wilderness_elapsed_minutes: 90,
            camp_destination: None,
            camp_remaining_minutes: 30,
            physiology_target: 2.0,
            command_target: 3.0,
            religion_target: 4.0,
        })
        .unwrap();
        assert_eq!(party.gateway_bucket, 5);
        assert_eq!(party.current_case_site_id.as_deref(), Some("site:1"));

        let request = PartyActionRequestView::from(sats::PartyActionRequest {
            id: 11,
            gateway_bucket: 4,
            party_id: "party:7".into(),
            requester_id: 7,
            action_kind: "travel".into(),
            summary: "Travel".into(),
            payload: "{}".into(),
        });
        assert_eq!((request.id, request.gateway_bucket), (11, 4));

        let route = PartyJourneyRouteView::from(sats::PartyJourneyRoute {
            party_id: "party:7".into(),
            gateway_bucket: 3,
            package_digest: "a".repeat(64),
            weather_rules_version: 2,
            weather_interval_start: 10,
            precipitation: sats::JourneyPrecipitation::Rain,
            intensity_bps: 100,
            ground_moisture_bps: 200,
            snow_cover_bps: 300,
            distance_m: 400,
            minutes: 500,
            points: Vec::new(),
            spans: Vec::new(),
            return_route: None,
        });
        assert_eq!((route.gateway_bucket, route.minutes), (3, 500));

        let equipped = EquippedItemView::from(sats::CharacterEquippedItem {
            inventory_item_id: 21,
            character_id: 7,
            placement_id: "right_hand".into(),
        });
        assert_eq!((equipped.inventory_item_id, equipped.character_id), (21, 7));
        assert!(equipped.item_name.is_empty());

        let role = RecruitmentRoleView::from(sats::PartyRecruitmentRole {
            id: 31,
            party_id: "party:7".into(),
            purpose: sats::RecruitmentRolePurpose::Specialized,
            name: "Vanguard".into(),
            requirements: generated_role_requirements(),
            quantity: 2,
        });
        assert!(role.requirements.melee);
        assert_eq!(role.purpose, sats::RecruitmentRolePurpose::Specialized);
        assert_eq!(role.autoresolve_combat_power, 0);

        let server = MissionServerView::from(sats::TacticalServer {
            identity: sats::spacetimedb_sdk::Identity::ZERO,
            gateway_bucket: 2,
            mission_id: "mission:1".into(),
            scene_key: "forest".into(),
            party_id: "party:7".into(),
            addr: "127.0.0.1:3000".into(),
            cert_digest: "cert".into(),
            expected_party_members: 2,
            authorized_party_member_ids: vec![7, 8],
            required_enemy_kills: 3,
            enemy_difficulty: 4,
            enemy_combat_scale_bps: 5,
            countermeasure_multiplier_bps: 6,
            normalized_combat_power: 7,
            enemy_character_ids: vec![9],
            party_has_surprise: true,
        });
        assert_eq!(
            (server.gateway_bucket, server.status),
            (2, MissionStatus::Ready)
        );
        assert_eq!(server.character_id, None);

        let server_request = MissionServerRequestView::from(sats::TacticalServerRequest {
            mission_id: "mission:2".into(),
            gateway_bucket: 1,
            scene_key: "road".into(),
            party_id: "party:7".into(),
            requested_by: 7,
            longitude_e_7: 100,
            latitude_e_7: 200,
            absolute_minute: 300,
            lunar_phase_minute: 400,
            expected_party_members: 2,
            authorized_party_member_ids: vec![7, 8],
            required_enemy_kills: 9,
            enemy_difficulty: 10,
            enemy_combat_scale_bps: 11,
            countermeasure_multiplier_bps: 12,
            normalized_combat_power: 13,
            enemy_character_ids: vec![14],
            party_has_surprise: false,
        });
        assert_eq!(
            (
                server_request.gateway_bucket,
                server_request.required_enemy_kills
            ),
            (1, 9)
        );

        let item = CatalogItemView::from(generated_item());
        assert_eq!(
            (item.kind, item.slot),
            (CatalogItemKind::Weapon, Slot::AnyHolding)
        );
        assert_eq!(item.equipment_placements[0].occupancy[0].order, 2);
        assert_eq!(item.equipment_placements[0].parents[0].order, 3);
        assert_eq!(item.attachment_points[0].accepts_tags, ["charm"]);
        assert_eq!(item.weapon_skills.sword, 1.0);
        assert_eq!(item.preferred_melee_style, MeleeAttackStyle::Stab);
        assert_eq!(item.moment_of_inertia_kg_m_2, 1.2);
        assert_eq!(
            (
                item.durability_yield,
                item.durability_fracture,
                item.durability_wear,
                item.durability_failure_share,
                item.edge_sensitivity,
                item.handling_sensitivity,
            ),
            (0.11, 0.22, 0.33, 0.44, 0.55, 0.66)
        );
    }

    #[test]
    fn personality_conversion_maps_every_axis_family() {
        let row = sats::CharacterPersonality {
            character_id: 7,
            projection_character_id: 9,
            nerve: sats::Nerve::Brave,
            drive: sats::Drive::Ambitious,
            outlook: sats::Outlook::Brooding,
            sociability: sats::Sociability::Gregarious,
            conscience: sats::Conscience::Cruel,
            self_regard: sats::SelfRegard::Humble,
            conviction: sats::Conviction::Irreverent,
            hygiene: sats::Hygiene::Cleanly,
            temperance: sats::Temperance::Drunkard,
            mirth: sats::Mirth::Merry,
            courtship: sats::Courtship::Proper,
            transparency: sats::Transparency::Guarded,
            self_knowledge: sats::SelfKnowledge::SelfDeceiving,
            sex: sats::Sex::Female,
            presentation: sats::Presentation::Woman,
            inclination: sats::Inclination::Neither,
        };
        let mapped = core_personality(&row);
        assert_eq!(mapped.nerve, Nerve::Brave);
        assert_eq!(mapped.self_knowledge, SelfKnowledge::SelfDeceiving);
        assert_eq!(mapped.inclination, Inclination::Neither);
    }

    #[test]
    fn generated_nested_rows_serialize_strictly() {
        let languages = sats::SettlementLanguageProfile {
            east_central_bp: 5_000,
            west_central_bp: 3_000,
            low_bp: 2_000,
            yiddish_incidence_bp: 100,
        };
        let converted: adventuresim_world_schema::SettlementLanguageProfile =
            sats_to_serde(&languages).unwrap();
        assert_eq!(converted.east_central_bp, 5_000);
        let mut encoded = serde_json::to_value(SerdeWrapper::from_ref(&languages)).unwrap();
        encoded["unexpected"] = serde_json::json!(1);
        assert!(
            serde_json::from_value::<SerdeWrapper<sats::SettlementLanguageProfile>>(encoded)
                .is_err()
        );
    }

    #[test]
    fn gateway_fields_are_removed_only_by_explicit_projection() {
        let row = sats::PartyActionRequest {
            id: 11,
            gateway_bucket: 4,
            party_id: "party:1".into(),
            requester_id: 7,
            action_kind: "travel".into(),
            summary: "Travel".into(),
            payload: "{}".into(),
        };
        let view = PartyActionRequestView::from(row);
        assert_eq!((view.id, view.party_id.as_str()), (11, "party:1"));
    }

    #[test]
    fn typed_investigation_availability_is_wording_invariant() {
        let generated = sats::InvestigationActionAvailability::Unavailable(
            sats::InvestigationActionUnavailableFields {
                reason: sats::InvestigationActionUnavailableReason::TravelRequired,
                can_travel_to_required_site: true,
                wait_minutes: 45,
            },
        );
        let outcome = core_investigation_action_availability(&generated);
        let (can_travel, wait_minutes) = match outcome {
            InvestigationActionAvailability::Available => (false, 0),
            InvestigationActionAvailability::Unavailable {
                reason: InvestigationActionUnavailableReason::TravelRequired,
                can_travel_to_required_site,
                wait_minutes,
            } => (can_travel_to_required_site, wait_minutes),
            InvestigationActionAvailability::Unavailable { .. } => panic!("wrong reason"),
        };
        assert!(can_travel);
        assert_eq!(wait_minutes, 45);
    }
}
