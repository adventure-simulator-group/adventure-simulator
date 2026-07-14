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
    pub temporary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub id: String,
    pub name: String,
    pub coord_x: f64,
    pub coord_y: f64,
    pub population_level: i32,
    pub population_estimate: u32,
    pub scene_key: String,
    pub source_node_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TravelEdge {
    pub id: u64,
    pub from_node_id: u64,
    pub to_node_id: u64,
    pub kind: String,
    pub length_m: u32,
    pub slope_multiplier: f32,
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
    pub status: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Party {
    pub id: String,
    pub name: String,
    pub leader_id: u64,
    pub current_settlement_id: Option<String>,
    pub current_quest_location_id: Option<String>,
    pub active_quest_id: Option<String>,
    pub is_solo: bool,
    pub medicine_target: f32,
    pub surgery_target: f32,
    pub charisma_target: f32,
    pub faith_target: f32,
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
pub struct PartyJoinRequest {
    pub id: u64,
    pub party_id: String,
    pub recruitment_role_id: u64,
    pub character_id: u64,
    pub meets_requirements: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterEquip {
    pub character_id: u64,
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

#[derive(Debug, Clone, Deserialize)]
pub struct ItemDefinition {
    pub id: String,
    pub weight: f32,
    pub kind: ItemKind,
    #[serde(default)]
    pub base_value: Option<u32>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterTime {
    pub character_id: u64,
    pub minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterTrainingSchedule {
    pub character_id: u64,
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
    pub labor_minutes: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldClock {
    pub id: u64,
    pub official_minutes: u64,
    pub epoch_micros: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStats {
    pub character_id: u64,
    pub calories_used: f32,
    pub focus: f32,
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
pub struct TacticalServer {
    #[serde(default)]
    pub identity: Option<String>,
    pub mission_id: String,
    pub scene_key: String,
    #[serde(default = "ready_status")]
    pub status: String,
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
            status: "Pending".to_string(),
            addr: String::new(),
            cert_digest: String::new(),
            character_id: Some(character_id),
            party_id,
        }
    }
}

fn ready_status() -> String {
    "Ready".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalServerRequest {
    pub mission_id: String,
    pub scene_key: String,
}
