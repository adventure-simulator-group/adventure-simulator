use serde::{Deserialize, Serialize};

/// Minimal quest status for demo purposes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuestStatus {
    Available,
    Active,
    Completed,
}

/// Minimal quest shape for simple demos (e.g., "pet the cat").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quest {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: QuestStatus,
    pub reward: Option<String>,
}
