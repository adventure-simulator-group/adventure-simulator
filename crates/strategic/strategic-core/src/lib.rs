use serde::{Deserialize, Serialize};

/// Minimal quest status for early strategic-layer demos.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuestStatus {
    Available,
    Active,
    Completed,
}

/// Minimal quest definition shape for early demos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quest {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: QuestStatus,
    pub reward: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CharacterLifeState {
    Alive,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub hp_current: i32,
    pub hp_max: i32,
    pub life: CharacterLifeState,
    pub deaths: i32,
    pub xp: i32,
    /// Unix epoch milliseconds at which respawn is allowed.
    pub respawn_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub item_id: String,
    pub qty: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardGrant {
    pub xp: i32,
    pub items: Vec<InventoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LootBag {
    pub id: String,
    pub character_id: String,
    pub created_at_ms: i64,
    pub world_pos: Option<[f32; 3]>,
    pub items: Vec<InventoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterState {
    pub character: Character,
    pub inventory: Vec<InventoryItem>,
    pub quest: Option<Quest>,
}
