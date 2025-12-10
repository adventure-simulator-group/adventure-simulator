use sqlx::Row;
use strategic_core::{Quest, QuestStatus};

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
}
