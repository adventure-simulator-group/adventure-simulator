//! Database rows and HTTP DTOs used by the strategic layer.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Character {
    pub id: i64,
    pub name: String,
    pub xp: i64,
    pub level: i64,
    pub gold: i64,
    pub current_settlement_id: Option<String>,
    pub party_id: Option<String>,
    pub active_mission_id: Option<String>,
    pub in_mission: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CharacterAttributes {
    pub character_id: i64,
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub strength: f32,
    pub precision: f32,
    pub agility: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CharacterStats {
    pub character_id: i64,
    pub calories_used: f32,
    pub focus: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CharacterSkills {
    pub character_id: i64,
    pub melee_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CharacterLimbs {
    pub character_id: i64,
    pub left_arm: f32,
    pub right_arm: f32,
    pub left_leg: f32,
    pub right_leg: f32,
    pub head: f32,
    pub chest: f32,
    pub stomach: f32,
}

#[derive(Debug, Clone, FromRow)]
pub struct CharacterEquip {
    pub character_id: i64,
    pub left_hand_item_id: Option<i64>,
    pub right_hand_item_id: Option<i64>,
    pub left_arm_armor_id: Option<i64>,
    pub right_arm_armor_id: Option<i64>,
    pub left_leg_armor_id: Option<i64>,
    pub right_leg_armor_id: Option<i64>,
    pub head_armor_id: Option<i64>,
    pub chest_armor_id: Option<i64>,
    pub stomach_armor_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InventoryItem {
    pub id: i64,
    pub character_id: i64,
    pub item_id: String,
    #[serde(alias = "quantity")]
    #[sqlx(rename = "quantity")]
    pub qty: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Settlement {
    pub id: String,
    pub name: String,
    pub coord_x: f64,
    pub coord_y: f64,
    pub population_level: i32,
    pub scene_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Quest {
    pub id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32,
    pub gold_reward: i64,
    pub xp_reward: i64,
    pub settlement_id: String,
    pub status: String,
    pub accepted_by: Option<String>,
    pub enemy_type: String,
    pub enemy_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Party {
    pub id: String,
    pub name: String,
    pub leader_id: i64,
    pub current_settlement_id: Option<String>,
    pub active_quest_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PartyMember {
    pub id: i64,
    pub party_id: String,
    pub character_id: i64,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TacticalServer {
    #[sqlx(rename = "id")]
    pub mission_id: String,
    pub scene_key: String,
    pub status: String,
    #[sqlx(default)]
    pub addr: String,
    #[sqlx(default)]
    pub cert_digest: String,
    #[sqlx(rename = "requester_character_id")]
    pub character_id: Option<i64>,
    pub party_id: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct MissionRecord {
    pub id: String,
    pub scene_key: String,
    pub status: String,
    pub party_id: Option<String>,
    pub quest_id: Option<String>,
    pub requester_character_id: Option<i64>,
    pub addr: Option<String>,
    pub cert_digest: Option<String>,
    pub pid: Option<i64>,
    pub success: Option<bool>,
    pub xp_gained: i64,
    pub result_committed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalCharacter {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalItem {
    pub id: String,
    pub weight: f32,
    pub slot: String,
    pub kind: String,
    pub accuracy: f32,
    pub block: f32,
    pub dodge: f32,
    pub coverage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedPlayerItem {
    pub quantity: u32,
    pub item: TacticalItem,
    pub equipped: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedPlayer {
    pub character: TacticalCharacter,
    pub items: Vec<ConnectedPlayerItem>,
    pub skills: CharacterSkills,
    pub stats: CharacterStats,
    pub attrs: CharacterAttributes,
    pub limbs: CharacterLimbs,
}

#[derive(Debug, Clone, FromRow)]
pub struct TacticalInventoryItemRow {
    pub inventory_item_id: i64,
    pub quantity: i64,
    pub item_id: String,
    pub weight: f32,
    pub slot: String,
    pub kind: String,
    pub accuracy: f32,
    pub block: f32,
    pub dodge: f32,
    pub coverage: f32,
}

impl CharacterEquip {
    pub fn equipped_slot(&self, inventory_item_id: i64) -> Option<&'static str> {
        [
            (self.left_hand_item_id, "LeftHolding"),
            (self.right_hand_item_id, "RightHolding"),
            (self.left_arm_armor_id, "LeftArm"),
            (self.right_arm_armor_id, "RightArm"),
            (self.left_leg_armor_id, "LeftLeg"),
            (self.right_leg_armor_id, "RightLeg"),
            (self.head_armor_id, "Head"),
            (self.chest_armor_id, "Chest"),
            (self.stomach_armor_id, "Stomach"),
        ]
        .into_iter()
        .find_map(|(id, slot)| (id == Some(inventory_item_id)).then_some(slot))
    }
}
