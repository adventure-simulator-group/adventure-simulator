use std::{collections::HashMap, sync::Arc};

use serde::Serialize;
use strategic_core::{
    Character, CharacterLifeState, InventoryItem, LootBag, Quest, QuestStatus, RewardGrant,
};
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Basic configuration for pushing events into a SpacetimeDB instance.
///
/// The `endpoint` should point at the HTTP endpoint exposed by your SpacetimeDB module
/// (for example, a mutation URL created with the hosted cloud console). When left empty,
/// the strategic layer falls back to an in-memory store so the demo keeps running without
/// any external services.
#[derive(Debug, Clone, Default)]
pub struct DbConfig {
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
}

impl DbConfig {
    pub fn from_env() -> Self {
        Self {
            endpoint: std::env::var("SPACETIME_ENDPOINT").ok(),
            api_key: std::env::var("SPACETIME_API_KEY").ok(),
        }
    }
}

#[derive(Clone)]
pub struct StrategicDb {
    state: Arc<RwLock<MemoryState>>,
    sink: Option<SpacetimeSink>,
}

#[derive(Default)]
struct MemoryState {
    quests: HashMap<String, Quest>,
    characters: HashMap<String, CharacterRecord>,
    loot_bags: HashMap<String, LootBagRecord>,
}

struct CharacterRecord {
    character: Character,
    inventory: HashMap<String, i32>,
    quests: HashMap<String, String>,
    updated_at_ms: i64,
}

struct LootBagRecord {
    bag: LootBag,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("character not found: {0}")]
    CharacterNotFound(String),
    #[error("quest not found: {0}")]
    QuestNotFound(String),
    #[error("spacetime push failed: {0}")]
    Spacetime(String),
}

pub type DbResult<T> = Result<T, DbError>;

impl StrategicDb {
    pub async fn connect(config: DbConfig) -> DbResult<Self> {
        let sink = SpacetimeSink::maybe_new(config).await?;
        Ok(Self {
            state: Arc::new(RwLock::new(MemoryState::default())),
            sink,
        })
    }

    async fn publish<T: Serialize>(&self, topic: &str, payload: &T) {
        if let Some(sink) = &self.sink {
            if let Err(err) = sink.publish(topic, payload).await {
                eprintln!("failed to push to spacetime: {err}");
            }
        }
    }

    pub async fn upsert_quest(&self, quest: &Quest) -> DbResult<()> {
        let mut state = self.state.write().await;
        state.quests.insert(quest.id.clone(), quest.clone());
        drop(state);

        self.publish("quests/upsert", quest).await;
        Ok(())
    }

    pub async fn get_quest(&self, id: &str) -> DbResult<Option<Quest>> {
        let state = self.state.read().await;
        Ok(state.quests.get(id).cloned())
    }

    pub async fn list_quests(&self) -> DbResult<Vec<Quest>> {
        let state = self.state.read().await;
        let mut quests = state.quests.values().cloned().collect::<Vec<_>>();
        quests.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(quests)
    }

    pub async fn complete_quest(&self, id: &str) -> DbResult<()> {
        let mut state = self.state.write().await;
        let Some(quest) = state.quests.get_mut(id) else {
            return Err(DbError::QuestNotFound(id.to_string()));
        };
        quest.status = QuestStatus::Completed;
        let quest = quest.clone();
        drop(state);

        self.publish("quests/complete", &quest).await;
        Ok(())
    }

    pub async fn start_quest(&self, id: &str) -> DbResult<()> {
        let mut state = self.state.write().await;
        let Some(quest) = state.quests.get_mut(id) else {
            return Err(DbError::QuestNotFound(id.to_string()));
        };
        quest.status = QuestStatus::Active;
        let quest = quest.clone();
        drop(state);

        self.publish("quests/start", &quest).await;
        Ok(())
    }

    pub async fn upsert_character(&self, character: &Character) -> DbResult<()> {
        let mut state = self.state.write().await;
        let record = state
            .characters
            .entry(character.id.clone())
            .or_insert_with(|| CharacterRecord {
                character: character.clone(),
                inventory: HashMap::new(),
                quests: HashMap::new(),
                updated_at_ms: now_ms(),
            });

        record.character = character.clone();
        record.updated_at_ms = now_ms();
        drop(state);

        self.publish("characters/upsert", character).await;
        Ok(())
    }

    pub async fn get_character(&self, id: &str) -> DbResult<Option<Character>> {
        let state = self.state.read().await;
        Ok(state.characters.get(id).map(|c| c.character.clone()))
    }

    pub async fn list_inventory(&self, character_id: &str) -> DbResult<Vec<InventoryItem>> {
        let state = self.state.read().await;
        let Some(record) = state.characters.get(character_id) else {
            return Ok(vec![]);
        };
        let mut items = record
            .inventory
            .iter()
            .map(|(item_id, qty)| InventoryItem {
                item_id: item_id.clone(),
                qty: *qty,
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.item_id.cmp(&b.item_id));
        Ok(items)
    }

    pub async fn add_item(&self, character_id: &str, item_id: &str, qty: i32) -> DbResult<()> {
        let mut state = self.state.write().await;
        let Some(record) = state.characters.get_mut(character_id) else {
            return Err(DbError::CharacterNotFound(character_id.to_string()));
        };
        if !matches!(record.character.life, CharacterLifeState::Alive) {
            return Ok(());
        }

        *record.inventory.entry(item_id.to_string()).or_default() += qty;
        record.updated_at_ms = now_ms();
        drop(state);

        self.publish(
            "inventory/add",
            &InventoryItem {
                item_id: item_id.to_string(),
                qty,
            },
        )
        .await;
        Ok(())
    }

    pub async fn start_character_quest(&self, character_id: &str, quest_id: &str) -> DbResult<()> {
        let mut state = self.state.write().await;
        if !state.quests.contains_key(quest_id) {
            return Err(DbError::QuestNotFound(quest_id.to_string()));
        }
        let Some(record) = state.characters.get_mut(character_id) else {
            return Err(DbError::CharacterNotFound(character_id.to_string()));
        };
        record
            .quests
            .insert(quest_id.to_string(), "active".to_string());
        record.updated_at_ms = now_ms();
        drop(state);

        self.publish(
            "quests/character/start",
            &serde_json::json!({
                "character_id": character_id,
                "quest_id": quest_id,
                "status": "active",
            }),
        )
        .await;
        Ok(())
    }

    pub async fn complete_character_quest(
        &self,
        character_id: &str,
        quest_id: &str,
    ) -> DbResult<()> {
        let mut state = self.state.write().await;
        if !state.quests.contains_key(quest_id) {
            return Err(DbError::QuestNotFound(quest_id.to_string()));
        }
        let Some(record) = state.characters.get_mut(character_id) else {
            return Err(DbError::CharacterNotFound(character_id.to_string()));
        };
        record
            .quests
            .insert(quest_id.to_string(), "completed".to_string());
        record.updated_at_ms = now_ms();
        drop(state);

        self.publish(
            "quests/character/complete",
            &serde_json::json!({
                "character_id": character_id,
                "quest_id": quest_id,
                "status": "completed",
            }),
        )
        .await;
        Ok(())
    }

    pub async fn get_character_quest_status(
        &self,
        character_id: &str,
        quest_id: &str,
    ) -> DbResult<Option<String>> {
        let state = self.state.read().await;
        let Some(record) = state.characters.get(character_id) else {
            return Ok(None);
        };
        Ok(record.quests.get(quest_id).cloned())
    }

    pub async fn list_character_quest_statuses(
        &self,
        character_id: &str,
    ) -> DbResult<Vec<(String, String)>> {
        let state = self.state.read().await;
        let Some(record) = state.characters.get(character_id) else {
            return Ok(vec![]);
        };
        let mut statuses = record
            .quests
            .iter()
            .map(|(q, s)| (q.clone(), s.clone()))
            .collect::<Vec<_>>();
        statuses.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(statuses)
    }

    pub async fn complete_character_quest_with_rewards(
        &self,
        character_id: &str,
        quest_id: &str,
        rewards: RewardGrant,
    ) -> DbResult<bool> {
        let mut state = self.state.write().await;
        if !state.quests.contains_key(quest_id) {
            return Err(DbError::QuestNotFound(quest_id.to_string()));
        }
        let Some(record) = state.characters.get_mut(character_id) else {
            return Err(DbError::CharacterNotFound(character_id.to_string()));
        };
        if !matches!(record.character.life, CharacterLifeState::Alive) {
            return Ok(false);
        }

        record
            .quests
            .insert(quest_id.to_string(), "completed".to_string());
        record.character.xp += rewards.xp;
        record.updated_at_ms = now_ms();
        for item in rewards.items.iter() {
            if item.qty == 0 {
                continue;
            }
            *record.inventory.entry(item.item_id.clone()).or_default() += item.qty;
        }
        let snapshot = record.character.clone();
        drop(state);

        self.publish(
            "quests/character/complete_with_rewards",
            &serde_json::json!({
                "character_id": character_id,
                "quest_id": quest_id,
                "rewards": rewards,
            }),
        )
        .await;
        Ok(matches!(snapshot.life, CharacterLifeState::Alive))
    }

    pub async fn apply_damage(
        &self,
        character_id: &str,
        amount: i32,
        respawn_delay_ms: i64,
        world_pos: Option<[f32; 3]>,
    ) -> DbResult<Character> {
        let mut state = self.state.write().await;
        let (snapshot, bag_to_insert) = {
            let Some(record) = state.characters.get_mut(character_id) else {
                return Err(DbError::CharacterNotFound(character_id.to_string()));
            };

            if !matches!(record.character.life, CharacterLifeState::Alive) {
                return Ok(record.character.clone());
            }

            let new_hp = (record.character.hp_current - amount).max(0);
            if new_hp > 0 {
                record.character.hp_current = new_hp;
                record.updated_at_ms = now_ms();
                let snapshot = record.character.clone();
                (snapshot, None)
            } else {
                // Death path.
                record.character.hp_current = 0;
                record.character.life = CharacterLifeState::Dead;
                record.character.deaths += 1;
                record.character.respawn_at_ms = Some(now_ms() + respawn_delay_ms);
                record.updated_at_ms = now_ms();

                let inventory = std::mem::take(&mut record.inventory);
                let bag_to_insert = if inventory.is_empty() {
                    None
                } else {
                    let bag_id = Uuid::new_v4().to_string();
                    let created_at_ms = now_ms();
                    let bag = LootBag {
                        id: bag_id.clone(),
                        character_id: character_id.to_string(),
                        created_at_ms,
                        world_pos,
                        items: inventory
                            .into_iter()
                            .map(|(item_id, qty)| InventoryItem { item_id, qty })
                            .collect(),
                    };
                    Some((bag_id, bag))
                };

                let snapshot = record.character.clone();
                (snapshot, bag_to_insert)
            }
        };

        if let Some((bag_id, bag)) = bag_to_insert {
            state
                .loot_bags
                .insert(bag_id.clone(), LootBagRecord { bag });
        }

        drop(state);

        if matches!(snapshot.life, CharacterLifeState::Alive) {
            self.publish("characters/damage", &snapshot).await;
        } else {
            self.publish("characters/death", &snapshot).await;
        }
        Ok(snapshot)
    }

    pub async fn list_loot_bags(&self, character_id: &str) -> DbResult<Vec<LootBag>> {
        let state = self.state.read().await;
        let mut bags = state
            .loot_bags
            .values()
            .filter(|bag| bag.bag.character_id == character_id)
            .map(|bag| bag.bag.clone())
            .collect::<Vec<_>>();
        bags.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
        bags.truncate(25);
        Ok(bags)
    }

    pub async fn claim_loot_bag(
        &self,
        loot_bag_id: &str,
        character_id: &str,
    ) -> DbResult<Vec<InventoryItem>> {
        let mut state = self.state.write().await;
        let Some(bag) = state.loot_bags.remove(loot_bag_id) else {
            return Ok(vec![]);
        };
        if bag.bag.character_id != character_id {
            // Return it to the map to keep state consistent.
            state.loot_bags.insert(loot_bag_id.to_string(), bag);
            return Ok(vec![]);
        }
        let Some(record) = state.characters.get_mut(character_id) else {
            state.loot_bags.insert(loot_bag_id.to_string(), bag);
            return Err(DbError::CharacterNotFound(character_id.to_string()));
        };
        if !matches!(record.character.life, CharacterLifeState::Alive) {
            state.loot_bags.insert(loot_bag_id.to_string(), bag);
            return Ok(vec![]);
        }

        for item in &bag.bag.items {
            *record.inventory.entry(item.item_id.clone()).or_default() += item.qty;
        }
        record.updated_at_ms = now_ms();
        let moved = bag.bag.items.clone();
        drop(state);

        self.publish(
            "loot/claim",
            &serde_json::json!({
                "character_id": character_id,
                "loot_bag_id": loot_bag_id,
            }),
        )
        .await;
        Ok(moved)
    }

    pub async fn respawn(&self, character_id: &str) -> DbResult<Character> {
        let mut state = self.state.write().await;
        let Some(record) = state.characters.get_mut(character_id) else {
            return Err(DbError::CharacterNotFound(character_id.to_string()));
        };
        let now = now_ms();

        if matches!(record.character.life, CharacterLifeState::Alive) {
            return Ok(record.character.clone());
        }

        if let Some(at) = record.character.respawn_at_ms {
            if now < at {
                return Ok(record.character.clone());
            }
        }

        record.character.life = CharacterLifeState::Alive;
        record.character.hp_current = record.character.hp_max;
        record.character.respawn_at_ms = None;
        record.updated_at_ms = now;
        *record.inventory.entry("bandage".to_string()).or_default() += 1;

        let snapshot = record.character.clone();
        drop(state);

        self.publish("characters/respawn", &snapshot).await;
        Ok(snapshot)
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_millis() as i64
}

#[derive(Clone)]
struct SpacetimeSink {
    client: reqwest::Client,
    endpoint: String,
    api_key: Option<String>,
}

impl SpacetimeSink {
    async fn maybe_new(config: DbConfig) -> DbResult<Option<Self>> {
        let Some(endpoint) = config.endpoint else {
            return Ok(None);
        };
        let client = reqwest::Client::builder()
            .user_agent("strategic-layer-spacetimedb")
            .build()
            .map_err(|e| DbError::Spacetime(e.to_string()))?;
        Ok(Some(Self {
            client,
            endpoint,
            api_key: config.api_key,
        }))
    }

    async fn publish<T: Serialize>(&self, topic: &str, payload: &T) -> DbResult<()> {
        let mut req = self
            .client
            .post(&self.endpoint)
            .json(&serde_json::json!({ "topic": topic, "payload": payload }));
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        req.send()
            .await
            .map_err(|e| DbError::Spacetime(e.to_string()))?
            .error_for_status()
            .map_err(|e| DbError::Spacetime(e.to_string()))?;
        Ok(())
    }
}
