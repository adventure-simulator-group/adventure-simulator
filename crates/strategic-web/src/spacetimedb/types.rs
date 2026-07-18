//! SpacetimeDB response types

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Response from SpacetimeDB SQL query (array of result sets)
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

// AlgebraicType can be many forms - we just need to accept any valid JSON
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum AlgebraicType {
    Value(serde_json::Value),
}

// Domain types matching strategic-db schema

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: u64,
    pub name: String,
    pub xp: u32,
    pub level: u32,
    pub gold: u32,
    pub current_settlement_id: Option<String>,
    pub current_quest_location_id: Option<String>,
    pub party_id: Option<String>,
    pub age_years: u16,
    pub alive: bool,
    pub temporary: bool,
}

macro_rules! personality_axis {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
    };
}
personality_axis!(Nerve {
    Neutral,
    Brave,
    Fearful
});
personality_axis!(Drive {
    Neutral,
    Ambitious,
    Content
});
personality_axis!(Outlook {
    Neutral,
    Sanguine,
    Brooding
});
personality_axis!(Sociability {
    Neutral,
    Gregarious,
    Solitary
});
personality_axis!(Conscience {
    Neutral,
    Compassionate,
    Callous,
    Cruel
});
personality_axis!(SelfRegard {
    Neutral,
    Proud,
    Humble
});
personality_axis!(Conviction {
    Neutral,
    Zealous,
    Irreverent
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterPersonality {
    pub character_id: u64,
    pub nerve: Nerve,
    pub drive: Drive,
    pub outlook: Outlook,
    pub sociability: Sociability,
    pub conscience: Conscience,
    pub self_regard: SelfRegard,
    pub conviction: Conviction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub id: String,
    pub name: String,
    pub coord_x: f64,
    pub coord_y: f64,
    pub population_level: i32,
    pub population_estimate: u32,
    pub industries: adventuresim_world_schema::InferredIndustryProfile,
    pub scene_key: String,
    pub religion_id: String,
    pub source_node_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementAlias {
    pub id: String,
    pub settlement_id: String,
    pub name: String,
    pub prefix: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementDescription {
    pub id: String,
    pub settlement_id: String,
    pub kind: SettlementDescriptionKind,
    pub language: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SettlementDescriptionKind {
    #[serde(alias = "settlement")]
    Settlement,
    #[serde(alias = "city")]
    City,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TravelEdge {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quest {
    pub id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32,
    pub gold_reward: i32,
    pub xp_reward: i32,
    pub settlement_id: String,
    pub status: QuestStatus,
    pub accepted_by: Option<String>,
    pub enemy_type: String,
    pub enemy_count: i32,
    pub location_description: String,
    pub location_scene_key: String,
    pub location_coord_x: f64,
    pub location_coord_y: f64,
    pub coordinates_are_geographic: bool,
    pub distance_m: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuestStatus {
    #[serde(alias = "available")]
    Available,
    #[serde(alias = "accepted")]
    Accepted,
    #[serde(alias = "completed")]
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestIssuer {
    pub quest_id: String,
    pub settlement_id: String,
    pub service_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Party {
    pub id: String,
    pub name: String,
    pub leader_id: u64,
    pub current_settlement_id: Option<String>,
    pub current_quest_location_id: Option<String>,
    pub active_quest_id: Option<String>,
    pub is_solo: bool,
    pub camp_fatigue_percent: u8,
    pub camp_destination_id: Option<String>,
    pub camp_destination_kind: Option<String>,
    pub camp_remaining_minutes: u64,
    pub medicine_target: f32,
    pub surgery_target: f32,
    pub charisma_target: f32,
    pub faith_target: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyJourney {
    pub party_id: String,
    pub origin_kind: String,
    pub origin_id: String,
    pub origin_name: String,
    pub destination_kind: String,
    pub destination_id: String,
    pub destination_name: String,
    pub total_minutes: u64,
    pub completed_minutes: u64,
    pub camp_stop_minutes: Vec<u64>,
    pub forecast_camp_stop_minutes: Vec<u64>,
    pub fatigue_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyMember {
    pub id: u64,
    pub party_id: String,
    pub character_id: u64,
    pub role: Option<String>,
    pub recruitment_role_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyActionRequest {
    pub id: u64,
    pub party_id: String,
    pub requester_id: u64,
    pub action_kind: String,
    pub summary: String,
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyLeaderVote {
    pub id: String,
    pub party_id: String,
    pub voter_id: u64,
    pub candidate_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalChatMessage {
    pub id: u64,
    pub conversation_key: String,
    pub sender_id: u64,
    pub sender_name: String,
    pub body: String,
    pub created_micros: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyInventoryItem {
    pub id: u64,
    pub party_id: String,
    pub item_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryQuantityTarget {
    pub id: String,
    pub owner_character_id: u64,
    pub party_scope: bool,
    pub item_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyStake {
    pub id: u64,
    pub party_id: String,
    pub character_id: u64,
    pub value: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleResult {
    pub quest_id: String,
    pub party_id: String,
    pub mission_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoresolveReport {
    pub quest_id: String,
    pub party_id: String,
    pub seed: u64,
    pub victor: String,
    pub rounds: u32,
    pub summary: String,
    pub log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleLootItem {
    pub id: u64,
    pub quest_id: String,
    pub item_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyJoinRequest {
    pub id: u64,
    pub party_id: String,
    pub recruitment_role_id: u64,
    pub character_id: u64,
    pub meets_requirements: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct RecruitmentRequirements {
    pub melee: bool,
    pub ranged: bool,
    pub precise: bool,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub athletics: u8,
    pub endurance: u8,
    pub medicine: u8,
    pub surgery: u8,
    pub charisma: u8,
    pub faith: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyRecruitmentRole {
    pub id: u64,
    pub party_id: String,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    pub quantity: u32,
    pub weapon_precision: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedRecruitmentRole {
    pub id: u64,
    pub owner_character_id: u64,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    pub weapon_precision: f32,
}

impl PartyRecruitmentRole {
    pub fn effective_weapon_precision(&self) -> f32 {
        self.weapon_precision
            .max(legacy_weapon_precision(self.requirements))
    }
}

impl SavedRecruitmentRole {
    pub fn effective_weapon_precision(&self) -> f32 {
        self.weapon_precision
            .max(legacy_weapon_precision(self.requirements))
    }
}

fn legacy_weapon_precision(requirements: RecruitmentRequirements) -> f32 {
    adventuresim_core::capability::legacy_weapon_precision(
        requirements.precise,
        requirements.blunt,
        requirements.slash,
        requirements.pierce,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCapability {
    pub character_id: u64,
    pub melee: bool,
    pub ranged: bool,
    pub precise: bool,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub athletics: f32,
    pub endurance: f32,
    pub medicine: f32,
    pub surgery: f32,
    pub charisma: f32,
    pub faith: f32,
    pub weapon_precision: f32,
}

impl CharacterCapability {
    pub fn summary_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        for (enabled, label) in [
            (self.melee, "Melee"),
            (self.ranged, "Ranged"),
            (self.heavy, "Heavy"),
        ] {
            if enabled {
                tags.push(label.into());
            }
        }
        if let Some(label) =
            adventuresim_core::capability::weapon_precision_tier_label(self.weapon_precision)
        {
            tags.push(label.into());
        }
        if self.full_armor {
            tags.push("Full armor".into());
        } else if self.three_quarter_armor {
            tags.push("3/4 armor".into());
        } else if self.half_armor {
            tags.push("1/2 armor".into());
        } else if self.quarter_armor {
            tags.push("1/4 armor".into());
        }
        for (value, label) in [
            (self.athletics, "Athletics"),
            (self.endurance, "Endurance"),
            (self.medicine, "Medicine"),
            (self.surgery, "Surgery"),
            (self.charisma, "Charisma"),
            (self.faith, "Faith"),
        ] {
            if adventuresim_core::capability::rating(value)
                >= adventuresim_core::capability::DEFAULT_NUMERIC_REQUIREMENT
            {
                tags.push(label.into());
            }
        }
        tags
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: u64,
    pub character_id: u64,
    pub item_id: String,
    #[serde(alias = "quantity")]
    pub qty: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterNeeds {
    pub character_id: u64,
    pub food_balance_kcal: f32,
    pub water_balance_ml: f32,
    pub carried_water_ml: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterEquip {
    #[serde(rename = "character_id")]
    pub _character_id: u64,
    pub left_hand_item_id: Option<u64>,
    pub right_hand_item_id: Option<u64>,
    pub left_arm_armor_id: Option<u64>,
    pub right_arm_armor_id: Option<u64>,
    pub left_leg_armor_id: Option<u64>,
    pub right_leg_armor_id: Option<u64>,
    pub head_armor_id: Option<u64>,
    pub chest_armor_id: Option<u64>,
    pub stomach_armor_id: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ItemDefinition {
    pub id: String,
    pub weight: f32,
    #[serde(default)]
    pub slot: ItemSlot,
    pub kind: ItemKind,
    #[serde(default)]
    pub base_value: Option<u32>,
    #[serde(default)]
    pub nutrition_kcal: f32,
    #[serde(default)]
    pub water_capacity_ml: u32,
    #[serde(default)]
    pub quality: u8,
    #[serde(default)]
    pub durability_yield: f32,
    #[serde(default)]
    pub durability_fracture: f32,
    #[serde(default)]
    pub durability_wear: f32,
    #[serde(default)]
    pub durability_failure_share: f32,
    #[serde(default)]
    pub edge_sensitivity: f32,
    #[serde(default)]
    pub handling_sensitivity: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemCondition {
    pub inventory_item_id: u64,
    pub tier_1: f32,
    pub tier_2: f32,
    pub tier_3: f32,
    pub tier_4: f32,
    pub tier_5: f32,
}

impl ItemCondition {
    pub fn bins(&self) -> [f32; 5] {
        [
            self.tier_1,
            self.tier_2,
            self.tier_3,
            self.tier_4,
            self.tier_5,
        ]
    }
    pub fn total(&self) -> f32 {
        self.bins().iter().sum::<f32>().clamp(0.0, 1.0)
    }
    pub fn repairable(&self, skill: u8) -> f32 {
        self.bins().iter().take(skill.min(5) as usize).sum()
    }
    pub fn residual(&self, skill: u8) -> f32 {
        self.bins().iter().skip(skill.min(5) as usize).sum()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct SettlementSmith {
    pub settlement_id: String,
    pub weaponsmith_skill: u8,
    pub armourer_skill: u8,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct RepairOrder {
    pub id: u64,
    pub owner_character_id: u64,
    pub inventory_item_id: u64,
    pub item_id: String,
    pub settlement_id: String,
    pub smith_skill: u8,
    pub submitted_at_minutes: u64,
    pub ready_at_minutes: u64,
    pub target_condition: f32,
    pub quoted_cost: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ItemSlot {
    #[default]
    None,
    LeftHolding,
    RightHolding,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Stomach,
    Head,
    AnyHolding,
    AnyArm,
    AnyLeg,
}

impl ItemSlot {
    pub fn sats_json(self) -> serde_json::Value {
        let tag = match self {
            Self::None => "none",
            Self::LeftHolding => "leftHolding",
            Self::RightHolding => "rightHolding",
            Self::LeftArm => "leftArm",
            Self::RightArm => "rightArm",
            Self::LeftLeg => "leftLeg",
            Self::RightLeg => "rightLeg",
            Self::Chest => "chest",
            Self::Stomach => "stomach",
            Self::Head => "head",
            Self::AnyHolding => "anyHolding",
            Self::AnyArm => "anyArm",
            Self::AnyLeg => "anyLeg",
        };
        serde_json::json!({ (tag): {} })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ItemKind {
    #[serde(alias = "Simple", alias = "simple")]
    Simple,
    #[serde(alias = "Weapon", alias = "weapon")]
    Weapon,
    #[serde(alias = "Armor", alias = "armor")]
    Armor,
    #[serde(alias = "Shield", alias = "shield")]
    Shield,
    #[serde(alias = "Clothing", alias = "clothing")]
    Clothing,
    #[serde(alias = "Currency", alias = "currency")]
    Currency,
}

/// Attribute values for a character. These mirror the public strategic tables
/// and are rendered as the base values on the character sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterAttributes {
    pub character_id: u64,
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub precision: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub left_arm_strength: f32,
    pub right_arm_strength: f32,
    pub left_leg_strength: f32,
    pub right_leg_strength: f32,
    pub left_arm_agility: f32,
    pub right_arm_agility: f32,
    pub left_leg_agility: f32,
    pub right_leg_agility: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterSkills {
    pub character_id: u64,
    pub melee_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
    pub ranged_hours: f32,
    pub will_hours: f32,
    pub charisma_hours: f32,
    pub medicine_hours: f32,
    pub faith_hours: f32,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub surgeon_hours: f32,
    pub smithing_hours: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterTime {
    pub character_id: u64,
    pub minutes: u64,
}

/// Queried only by strategic-web and immediately sanitized. Browser responses
/// never serialize this private disease row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfectionEpisodeRow {
    pub id: u64,
    pub character_id: u64,
    pub disease_id: String,
    pub contracted_at: u64,
    pub treated_at: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommittedCutRow {
    pub id: u64,
    pub character_id: u64,
    pub committed_at: u64,
    pub severity: f32,
    pub surgery_check: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStats {
    pub character_id: u64,
    pub calories_used: f32,
    pub focus: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScheduleAllocation {
    pub melee_minutes: u16,
    pub dodge_minutes: u16,
    pub block_minutes: u16,
    pub ranged_minutes: u16,
    pub will_minutes: u16,
    pub charisma_minutes: u16,
    pub medicine_minutes: u16,
    pub faith_minutes: u16,
    pub stealth_minutes: u16,
    pub balance_minutes: u16,
    pub surgeon_minutes: u16,
    pub smithing_minutes: u16,
    pub labor_minutes: u16,
    pub prayer_minutes: u16,
    pub thievery_minutes: u16,
    pub raiding_minutes: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterTrainingSchedule {
    pub character_id: u64,
    pub downtime: ScheduleAllocation,
    /// Legacy compatibility field; strategic travel no longer trains skills.
    pub travel: ScheduleAllocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterNotoriety {
    pub character_id: u64,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldClock {
    pub id: u64,
    pub official_minutes: u64,
    pub epoch_micros: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterLimbs {
    pub character_id: u64,
    pub left_arm_health: f32,
    pub right_arm_health: f32,
    pub left_leg_health: f32,
    pub right_leg_health: f32,
    pub head_health: f32,
    pub chest_health: f32,
    pub stomach_health: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCondition {
    pub character_id: u64,
    pub body_weight_kg: f32,
    pub current_blood_ml: f32,
    pub maximum_blood_ml: f32,
    pub religion_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStrategicCondition {
    pub character_id: u64,
    pub morale: f32,
    pub morale_bonus: f32,
    pub morale_bonus_cap: f32,
    pub fervor: f32,
    pub pain: f32,
    pub blood_loss: f32,
    pub fear: f32,
    pub fatigue: f32,
    pub hunger: f32,
    pub thirst: f32,
    pub food_days: f32,
    pub water_days: f32,
    pub water_capacity_ml: u32,
    pub incapacitation: f32,
    pub check_multiplier: f32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterMoraleSource {
    pub id: String,
    pub character_id: u64,
    pub kind: String,
    pub label: String,
    pub magnitude: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReligiousDemand {
    pub id: u64,
    pub character_id: u64,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub fervor: f32,
    pub status: String,
    pub created_at_minute: u64,
    pub resolved_at_minute: Option<u64>,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalServer {
    #[serde(default)]
    pub identity: Option<String>,
    pub mission_id: String,
    pub scene_key: String,
    #[serde(default)]
    pub status: MissionStatus,
    #[serde(default)]
    pub addr: String,
    #[serde(default)]
    pub cert_digest: String,
    #[serde(default)]
    pub character_id: Option<u64>,
    #[serde(default)]
    pub party_id: Option<String>,
}

impl TacticalServer {
    pub fn pending(
        mission_id: String,
        scene_key: String,
        character_id: u64,
        party_id: Option<String>,
    ) -> Self {
        Self {
            identity: None,
            mission_id,
            scene_key,
            status: MissionStatus::Pending,
            addr: String::new(),
            cert_digest: String::new(),
            character_id: Some(character_id),
            party_id,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MissionStatus {
    #[default]
    #[serde(alias = "Ready", alias = "ready")]
    Ready,
    #[serde(
        alias = "Pending",
        alias = "pending",
        alias = "Requested",
        alias = "requested",
        alias = "Starting",
        alias = "starting"
    )]
    Pending,
    #[serde(alias = "Failed", alias = "failed", alias = "Error", alias = "error")]
    Failed,
    #[serde(alias = "Ended", alias = "ended", alias = "Stopped", alias = "stopped")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalServerRequest {
    pub mission_id: String,
    pub scene_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategic_statuses_reject_unknown_values() {
        assert_eq!(
            serde_json::from_str::<QuestStatus>("\"accepted\"").unwrap(),
            QuestStatus::Accepted
        );
        assert!(serde_json::from_str::<QuestStatus>("\"mystery\"").is_err());
        assert_eq!(
            serde_json::from_str::<MissionStatus>("\"Starting\"").unwrap(),
            MissionStatus::Pending
        );
    }

    #[test]
    fn settlement_description_kind_is_a_closed_set() {
        assert_eq!(
            serde_json::from_str::<SettlementDescriptionKind>("\"city\"").unwrap(),
            SettlementDescriptionKind::City
        );
        assert!(serde_json::from_str::<SettlementDescriptionKind>("\"bridge\"").is_err());
    }

    #[test]
    fn item_slots_use_sats_tagged_sum_arguments() {
        assert_eq!(
            ItemSlot::None.sats_json(),
            serde_json::json!({ "none": {} })
        );
        assert_eq!(
            ItemSlot::AnyHolding.sats_json(),
            serde_json::json!({ "anyHolding": {} })
        );
    }
}
