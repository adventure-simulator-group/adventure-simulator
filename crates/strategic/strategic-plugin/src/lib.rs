use bevy::prelude::*;
use reqwest::Client;
use serde::de::DeserializeOwned;
use strategic_core::{Character, InventoryItem, Quest, RewardGrant};

/// Bevy plugin that injects a simple HTTP client for the strategic layer.
pub struct StrategicPlugin {
    pub base_url: String,
}

impl Default for StrategicPlugin {
    fn default() -> Self {
        Self {
            base_url: std::env::var("STRATEGIC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string()),
        }
    }
}

impl Plugin for StrategicPlugin {
    fn build(&self, app: &mut App) {
        let client = StrategicClient::new(self.base_url.clone());
        app.insert_resource(client);
    }
}

#[derive(Resource, Clone)]
pub struct StrategicClient {
    base_url: String,
    http: Client,
}

impl StrategicClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http: Client::new(),
        }
    }

    /// Fetch a quest by id.
    pub async fn fetch_quest(&self, quest_id: &str) -> reqwest::Result<Quest> {
        self.get(&format!("/quests/{quest_id}")).await
    }

    pub async fn get_character(&self, character_id: &str) -> reqwest::Result<Character> {
        self.get(&format!("/api/characters/{character_id}")).await
    }

    pub async fn get_inventory(&self, character_id: &str) -> reqwest::Result<Vec<InventoryItem>> {
        self.get(&format!("/api/characters/{character_id}/inventory")).await
    }

    pub async fn start_quest(&self, character_id: &str, quest_id: &str) -> reqwest::Result<()> {
        let url = format!(
            "{}/api/characters/{character_id}/quests/{quest_id}/start",
            self.base_url
        );
        self.http.post(url).send().await?.error_for_status()?;
        Ok(())
    }

    pub async fn complete_quest(
        &self,
        character_id: &str,
        quest_id: &str,
        rewards: Option<RewardGrant>,
    ) -> reqwest::Result<()> {
        let url = format!(
            "{}/api/characters/{character_id}/quests/{quest_id}/complete",
            self.base_url
        );
        let req = self.http.post(url);
        let req = if let Some(r) = rewards { req.json(&r) } else { req };
        req.send().await?.error_for_status()?;
        Ok(())
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> reqwest::Result<T> {
        let url = format!("{}{}", self.base_url, path);
        self.http.get(url).send().await?.error_for_status()?.json().await
    }
}
