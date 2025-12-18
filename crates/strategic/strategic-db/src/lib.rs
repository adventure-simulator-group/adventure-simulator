use sqlx::Row;
use strategic_core::{Character, CharacterLifeState, InventoryItem, LootBag, Quest, QuestStatus, RewardGrant};
use uuid::Uuid;

pub use sqlx::{postgres::PgPoolOptions, PgPool};

/// Lightweight database facade for the demo.
#[derive(Clone)]
pub struct StrategicDb {
    pool: PgPool,
}

impl StrategicDb {
    /// Create a new connection pool.
    pub async fn connect(database_url: &str, max_connections: u32) -> sqlx::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Create the minimal schema if it does not already exist.
    pub async fn ensure_schema(&self) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS quests (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                status TEXT NOT NULL,
                data JSONB NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS characters (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                hp_current INT NOT NULL,
                hp_max INT NOT NULL,
                alive BOOL NOT NULL,
                deaths INT NOT NULL DEFAULT 0,
                xp INT NOT NULL DEFAULT 0,
                respawn_at_ms BIGINT NULL,
                updated_at_ms BIGINT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS inventory_items (
                character_id TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
                item_id TEXT NOT NULL,
                qty INT NOT NULL,
                PRIMARY KEY (character_id, item_id),
                CHECK (qty >= 0)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS character_quests (
                character_id TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
                quest_id TEXT NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
                status TEXT NOT NULL,
                updated_at_ms BIGINT NOT NULL,
                PRIMARY KEY (character_id, quest_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS loot_bags (
                id TEXT PRIMARY KEY,
                character_id TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
                created_at_ms BIGINT NOT NULL,
                world_x REAL NULL,
                world_y REAL NULL,
                world_z REAL NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS loot_bag_items (
                loot_bag_id TEXT NOT NULL REFERENCES loot_bags(id) ON DELETE CASCADE,
                item_id TEXT NOT NULL,
                qty INT NOT NULL,
                PRIMARY KEY (loot_bag_id, item_id),
                CHECK (qty >= 0)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Insert or update a quest row.
    pub async fn upsert_quest(&self, quest: &Quest) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO quests (id, title, description, status, data)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, description = EXCLUDED.description, status = EXCLUDED.status, data = EXCLUDED.data
            "#,
        )
        .bind(&quest.id)
        .bind(&quest.title)
        .bind(&quest.description)
        .bind(Self::status_to_str(&quest.status))
        .bind(
            serde_json::to_value(quest)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fetch a quest by id.
    pub async fn get_quest(&self, id: &str) -> sqlx::Result<Option<Quest>> {
        let rec = sqlx::query(r#"SELECT data FROM quests WHERE id = $1"#)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        let Some(row) = rec else { return Ok(None); };
        let data: serde_json::Value = row.try_get("data")?;
        let quest: Quest =
            serde_json::from_value(data).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok(Some(quest))
    }

    pub async fn list_quests(&self) -> sqlx::Result<Vec<Quest>> {
        let rows = sqlx::query(r#"SELECT data FROM quests ORDER BY id ASC"#)
            .fetch_all(&self.pool)
            .await?;

        let mut quests = Vec::with_capacity(rows.len());
        for row in rows {
            let data: serde_json::Value = row.try_get("data")?;
            let quest: Quest =
                serde_json::from_value(data).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            quests.push(quest);
        }
        Ok(quests)
    }

    /// Mark a quest as completed.
    pub async fn complete_quest(&self, id: &str) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            UPDATE quests SET status = $2, data = jsonb_set(data, '{status}', to_jsonb($2::text))
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(Self::status_to_str(&QuestStatus::Completed))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a quest as active/started.
    pub async fn start_quest(&self, id: &str) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            UPDATE quests SET status = $2, data = jsonb_set(data, '{status}', to_jsonb($2::text))
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(Self::status_to_str(&QuestStatus::Active))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    fn status_to_str(status: &QuestStatus) -> &str {
        match status {
            QuestStatus::Available => "available",
            QuestStatus::Active => "active",
            QuestStatus::Completed => "completed",
        }
    }

    pub async fn upsert_character(&self, character: &Character) -> sqlx::Result<()> {
        let (alive, respawn_at_ms) = match character.life {
            CharacterLifeState::Alive => (true, None),
            CharacterLifeState::Dead => (false, character.respawn_at_ms),
        };

        sqlx::query(
            r#"
            INSERT INTO characters (id, name, hp_current, hp_max, alive, deaths, xp, respawn_at_ms, updated_at_ms)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (id) DO UPDATE
            SET name = EXCLUDED.name,
                hp_current = EXCLUDED.hp_current,
                hp_max = EXCLUDED.hp_max,
                alive = EXCLUDED.alive,
                deaths = EXCLUDED.deaths,
                xp = EXCLUDED.xp,
                respawn_at_ms = EXCLUDED.respawn_at_ms,
                updated_at_ms = EXCLUDED.updated_at_ms
            "#,
        )
        .bind(&character.id)
        .bind(&character.name)
        .bind(character.hp_current)
        .bind(character.hp_max)
        .bind(alive)
        .bind(character.deaths)
        .bind(character.xp)
        .bind(respawn_at_ms)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_character(&self, id: &str) -> sqlx::Result<Option<Character>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, hp_current, hp_max, alive, deaths, xp, respawn_at_ms
            FROM characters
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else { return Ok(None); };
        Ok(Some(Character {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            hp_current: row.try_get("hp_current")?,
            hp_max: row.try_get("hp_max")?,
            life: if row.try_get::<bool, _>("alive")? {
                CharacterLifeState::Alive
            } else {
                CharacterLifeState::Dead
            },
            deaths: row.try_get("deaths")?,
            xp: row.try_get("xp")?,
            respawn_at_ms: row.try_get("respawn_at_ms")?,
        }))
    }

    pub async fn list_inventory(&self, character_id: &str) -> sqlx::Result<Vec<InventoryItem>> {
        let rows = sqlx::query(
            r#"
            SELECT item_id, qty
            FROM inventory_items
            WHERE character_id = $1
            ORDER BY item_id ASC
            "#,
        )
        .bind(character_id)
        .fetch_all(&self.pool)
        .await?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(InventoryItem {
                item_id: row.try_get("item_id")?,
                qty: row.try_get("qty")?,
            });
        }
        Ok(items)
    }

    pub async fn add_item(&self, character_id: &str, item_id: &str, qty: i32) -> sqlx::Result<()> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(r#"SELECT alive FROM characters WHERE id = $1 FOR UPDATE"#)
            .bind(character_id)
            .fetch_one(&mut *tx)
            .await?;
        let alive: bool = row.try_get("alive")?;
        if !alive {
            tx.commit().await?;
            return Ok(());
        }

        sqlx::query(
            r#"
            INSERT INTO inventory_items (character_id, item_id, qty)
            VALUES ($1, $2, $3)
            ON CONFLICT (character_id, item_id) DO UPDATE
            SET qty = inventory_items.qty + EXCLUDED.qty
            "#,
        )
        .bind(character_id)
        .bind(item_id)
        .bind(qty)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE characters SET updated_at_ms = $2 WHERE id = $1")
            .bind(character_id)
            .bind(now_ms())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn start_character_quest(
        &self,
        character_id: &str,
        quest_id: &str,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO character_quests (character_id, quest_id, status, updated_at_ms)
            VALUES ($1, $2, 'active', $3)
            ON CONFLICT (character_id, quest_id) DO UPDATE
            SET status = 'active', updated_at_ms = EXCLUDED.updated_at_ms
            "#,
        )
        .bind(character_id)
        .bind(quest_id)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn complete_character_quest(
        &self,
        character_id: &str,
        quest_id: &str,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO character_quests (character_id, quest_id, status, updated_at_ms)
            VALUES ($1, $2, 'completed', $3)
            ON CONFLICT (character_id, quest_id) DO UPDATE
            SET status = 'completed', updated_at_ms = EXCLUDED.updated_at_ms
            "#,
        )
        .bind(character_id)
        .bind(quest_id)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_character_quest_status(
        &self,
        character_id: &str,
        quest_id: &str,
    ) -> sqlx::Result<Option<String>> {
        let row = sqlx::query(
            r#"SELECT status FROM character_quests WHERE character_id = $1 AND quest_id = $2"#,
        )
        .bind(character_id)
        .bind(quest_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<String, _>("status")))
    }

    pub async fn list_character_quest_statuses(
        &self,
        character_id: &str,
    ) -> sqlx::Result<Vec<(String, String)>> {
        let rows = sqlx::query(
            r#"
            SELECT quest_id, status
            FROM character_quests
            WHERE character_id = $1
            ORDER BY quest_id ASC
            "#,
        )
        .bind(character_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.get::<String, _>("quest_id"), row.get::<String, _>("status")))
            .collect())
    }

    pub async fn complete_character_quest_with_rewards(
        &self,
        character_id: &str,
        quest_id: &str,
        rewards: RewardGrant,
    ) -> sqlx::Result<bool> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(r#"SELECT alive FROM characters WHERE id = $1 FOR UPDATE"#)
            .bind(character_id)
            .fetch_one(&mut *tx)
            .await?;
        let alive: bool = row.try_get("alive")?;
        if !alive {
            tx.commit().await?;
            return Ok(false);
        }

        sqlx::query(
            r#"
            INSERT INTO character_quests (character_id, quest_id, status, updated_at_ms)
            VALUES ($1, $2, 'completed', $3)
            ON CONFLICT (character_id, quest_id) DO UPDATE
            SET status = 'completed', updated_at_ms = EXCLUDED.updated_at_ms
            "#,
        )
        .bind(character_id)
        .bind(quest_id)
        .bind(now_ms())
        .execute(&mut *tx)
        .await?;

        if rewards.xp != 0 {
            sqlx::query(r#"UPDATE characters SET xp = xp + $2, updated_at_ms = $3 WHERE id = $1"#)
                .bind(character_id)
                .bind(rewards.xp)
                .bind(now_ms())
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query(r#"UPDATE characters SET updated_at_ms = $2 WHERE id = $1"#)
                .bind(character_id)
                .bind(now_ms())
                .execute(&mut *tx)
                .await?;
        }

        for item in rewards.items {
            if item.qty == 0 {
                continue;
            }
            sqlx::query(
                r#"
                INSERT INTO inventory_items (character_id, item_id, qty)
                VALUES ($1, $2, $3)
                ON CONFLICT (character_id, item_id) DO UPDATE
                SET qty = inventory_items.qty + EXCLUDED.qty
                "#,
            )
            .bind(character_id)
            .bind(item.item_id)
            .bind(item.qty)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(true)
    }

    pub async fn apply_damage(
        &self,
        character_id: &str,
        amount: i32,
        respawn_delay_ms: i64,
        world_pos: Option<[f32; 3]>,
    ) -> sqlx::Result<Character> {
        let mut tx = self.pool.begin().await?;

        // Lock the row so concurrent damage/loot/XP updates serialize cleanly.
        let row = sqlx::query(
            r#"
            SELECT id, name, hp_current, hp_max, alive, deaths, xp, respawn_at_ms
            FROM characters
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(character_id)
        .fetch_one(&mut *tx)
        .await?;

        let hp_current: i32 = row.try_get("hp_current")?;
        let hp_max: i32 = row.try_get("hp_max")?;
        let alive: bool = row.try_get("alive")?;
        let mut deaths: i32 = row.try_get("deaths")?;
        let xp: i32 = row.try_get("xp")?;

        if !alive {
            // Already dead: no-op.
            tx.commit().await?;
            return Ok(Character {
                id: character_id.to_string(),
                name: row.try_get("name")?,
                hp_current,
                hp_max,
                life: CharacterLifeState::Dead,
                deaths,
                xp,
                respawn_at_ms: row.try_get("respawn_at_ms")?,
            });
        }

        let new_hp = (hp_current - amount).max(0);
        if new_hp > 0 {
            sqlx::query(
                r#"UPDATE characters SET hp_current = $2, updated_at_ms = $3 WHERE id = $1"#,
            )
            .bind(character_id)
            .bind(new_hp)
            .bind(now_ms())
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            return Ok(Character {
                id: character_id.to_string(),
                name: row.try_get("name")?,
                hp_current: new_hp,
                hp_max,
                life: CharacterLifeState::Alive,
                deaths,
                xp,
                respawn_at_ms: None,
            });
        }

        // Death path: mark dead, schedule respawn, and drop inventory.
        deaths += 1;
        let respawn_at_ms = now_ms() + respawn_delay_ms;
        sqlx::query(
            r#"
            UPDATE characters
            SET hp_current = 0,
                alive = false,
                deaths = $2,
                respawn_at_ms = $3,
                updated_at_ms = $4
            WHERE id = $1
            "#,
        )
        .bind(character_id)
        .bind(deaths)
        .bind(respawn_at_ms)
        .bind(now_ms())
        .execute(&mut *tx)
        .await?;

        // Move inventory into a loot bag atomically (prevents "half moved" states).
        let inv_rows = sqlx::query(
            r#"
            SELECT item_id, qty
            FROM inventory_items
            WHERE character_id = $1
            FOR UPDATE
            "#,
        )
        .bind(character_id)
        .fetch_all(&mut *tx)
        .await?;

        if !inv_rows.is_empty() {
            let loot_bag_id = Uuid::new_v4().to_string();
            let (world_x, world_y, world_z) = world_pos
                .map(|p| (Some(p[0]), Some(p[1]), Some(p[2])))
                .unwrap_or((None, None, None));

            sqlx::query(
                r#"
                INSERT INTO loot_bags (id, character_id, created_at_ms, world_x, world_y, world_z)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
            )
            .bind(&loot_bag_id)
            .bind(character_id)
            .bind(now_ms())
            .bind(world_x)
            .bind(world_y)
            .bind(world_z)
            .execute(&mut *tx)
            .await?;

            for row in inv_rows {
                let item_id: String = row.try_get("item_id")?;
                let qty: i32 = row.try_get("qty")?;
                sqlx::query(
                    r#"
                    INSERT INTO loot_bag_items (loot_bag_id, item_id, qty)
                    VALUES ($1, $2, $3)
                    "#,
                )
                .bind(&loot_bag_id)
                .bind(item_id)
                .bind(qty)
                .execute(&mut *tx)
                .await?;
            }

            sqlx::query(r#"DELETE FROM inventory_items WHERE character_id = $1"#)
                .bind(character_id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(Character {
            id: character_id.to_string(),
            name: row.try_get("name")?,
            hp_current: 0,
            hp_max,
            life: CharacterLifeState::Dead,
            deaths,
            xp,
            respawn_at_ms: Some(respawn_at_ms),
        })
    }

    pub async fn list_loot_bags(&self, character_id: &str) -> sqlx::Result<Vec<LootBag>> {
        let bags = sqlx::query(
            r#"
            SELECT id, character_id, created_at_ms, world_x, world_y, world_z
            FROM loot_bags
            WHERE character_id = $1
            ORDER BY created_at_ms DESC
            LIMIT 25
            "#,
        )
        .bind(character_id)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(bags.len());
        for bag in bags {
            let id: String = bag.try_get("id")?;
            let items_rows = sqlx::query(
                r#"
                SELECT item_id, qty
                FROM loot_bag_items
                WHERE loot_bag_id = $1
                ORDER BY item_id ASC
                "#,
            )
            .bind(&id)
            .fetch_all(&self.pool)
            .await?;

            let items = items_rows
                .into_iter()
                .map(|row| InventoryItem {
                    item_id: row.get::<String, _>("item_id"),
                    qty: row.get::<i32, _>("qty"),
                })
                .collect::<Vec<_>>();

            let world_x: Option<f32> = bag.try_get("world_x")?;
            let world_y: Option<f32> = bag.try_get("world_y")?;
            let world_z: Option<f32> = bag.try_get("world_z")?;
            let world_pos = match (world_x, world_y, world_z) {
                (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                _ => None,
            };

            out.push(LootBag {
                id,
                character_id: bag.get::<String, _>("character_id"),
                created_at_ms: bag.get::<i64, _>("created_at_ms"),
                world_pos,
                items,
            });
        }
        Ok(out)
    }

    pub async fn claim_loot_bag(
        &self,
        loot_bag_id: &str,
        character_id: &str,
    ) -> sqlx::Result<Vec<InventoryItem>> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(r#"SELECT alive FROM characters WHERE id = $1 FOR UPDATE"#)
            .bind(character_id)
            .fetch_one(&mut *tx)
            .await?;
        let alive: bool = row.try_get("alive")?;
        if !alive {
            tx.commit().await?;
            return Ok(vec![]);
        }

        let bag_row = sqlx::query(
            r#"
            SELECT character_id
            FROM loot_bags
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(loot_bag_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(bag_row) = bag_row else {
            tx.commit().await?;
            return Ok(vec![]);
        };
        let owner_id: String = bag_row.try_get("character_id")?;
        if owner_id != character_id {
            tx.commit().await?;
            return Ok(vec![]);
        }

        let item_rows = sqlx::query(
            r#"
            SELECT item_id, qty
            FROM loot_bag_items
            WHERE loot_bag_id = $1
            FOR UPDATE
            "#,
        )
        .bind(loot_bag_id)
        .fetch_all(&mut *tx)
        .await?;

        let mut moved = Vec::with_capacity(item_rows.len());
        for row in item_rows {
            let item_id: String = row.try_get("item_id")?;
            let qty: i32 = row.try_get("qty")?;
            moved.push(InventoryItem {
                item_id: item_id.clone(),
                qty,
            });
            sqlx::query(
                r#"
                INSERT INTO inventory_items (character_id, item_id, qty)
                VALUES ($1, $2, $3)
                ON CONFLICT (character_id, item_id) DO UPDATE
                SET qty = inventory_items.qty + EXCLUDED.qty
                "#,
            )
            .bind(character_id)
            .bind(item_id)
            .bind(qty)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(r#"DELETE FROM loot_bags WHERE id = $1"#)
            .bind(loot_bag_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("UPDATE characters SET updated_at_ms = $2 WHERE id = $1")
            .bind(character_id)
            .bind(now_ms())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(moved)
    }

    pub async fn respawn(&self, character_id: &str) -> sqlx::Result<Character> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            r#"
            SELECT id, name, hp_current, hp_max, alive, deaths, xp, respawn_at_ms
            FROM characters
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(character_id)
        .fetch_one(&mut *tx)
        .await?;

        let hp_max: i32 = row.try_get("hp_max")?;
        let alive: bool = row.try_get("alive")?;
        let respawn_at_ms: Option<i64> = row.try_get("respawn_at_ms")?;
        let now = now_ms();

        if alive {
            tx.commit().await?;
            return Ok(Character {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                hp_current: row.try_get("hp_current")?,
                hp_max,
                life: CharacterLifeState::Alive,
                deaths: row.try_get("deaths")?,
                xp: row.try_get("xp")?,
                respawn_at_ms: None,
            });
        }

        if let Some(at) = respawn_at_ms {
            if now < at {
                tx.commit().await?;
                return Ok(Character {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    hp_current: row.try_get("hp_current")?,
                    hp_max,
                    life: CharacterLifeState::Dead,
                    deaths: row.try_get("deaths")?,
                    xp: row.try_get("xp")?,
                    respawn_at_ms,
                });
            }
        }

        sqlx::query(
            r#"
            UPDATE characters
            SET alive = true,
                hp_current = hp_max,
                respawn_at_ms = NULL,
                updated_at_ms = $2
            WHERE id = $1
            "#,
        )
        .bind(character_id)
        .bind(now_ms())
        .execute(&mut *tx)
        .await?;

        // Starter item on respawn.
        sqlx::query(
            r#"
            INSERT INTO inventory_items (character_id, item_id, qty)
            VALUES ($1, 'bandage', 1)
            ON CONFLICT (character_id, item_id) DO UPDATE SET qty = inventory_items.qty + 1
            "#,
        )
        .bind(character_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Character {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            hp_current: hp_max,
            hp_max,
            life: CharacterLifeState::Alive,
            deaths: row.try_get("deaths")?,
            xp: row.try_get("xp")?,
            respawn_at_ms: None,
        })
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_millis() as i64
}
