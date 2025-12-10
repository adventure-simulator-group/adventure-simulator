use clap::Parser;
use strategic_db::StrategicDb;

/// Simple helper CLI for the strategic demo.
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Postgres connection string
    #[arg(long, default_value = "postgres://postgres:postgres@localhost:5432/strategic")]
    database_url: String,

    /// Quest id to upsert
    #[arg(long)]
    id: String,
    /// Quest title
    #[arg(long)]
    title: String,
    /// Quest description
    #[arg(long)]
    description: String,
    /// Optional reward text
    #[arg(long)]
    reward: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = Args::parse();
    if args.database_url.is_empty() {
        if let Ok(env_url) = std::env::var("DATABASE_URL") {
            args.database_url = env_url;
        }
    }
    let db = StrategicDb::connect(&args.database_url, 5).await?;
    db.ensure_schema().await?;
    let quest = strategic_core::Quest {
        id: args.id,
        title: args.title,
        description: args.description,
        status: strategic_core::QuestStatus::Available,
        reward: args.reward,
    };
    db.upsert_quest(&quest).await?;
    println!("Upserted quest {} at {}", quest.id, args.database_url);
    Ok(())
}
