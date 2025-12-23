use clap::Parser;
use strategic_db::StrategicDb;

/// Simple helper CLI for the strategic demo.
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Spacetime endpoint (mutation URL)
    #[arg(long, default_value = "")]
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
    let mut config = strategic_db::DbConfig::from_env();
    if !args.database_url.is_empty() {
        config.endpoint = Some(args.database_url.clone());
    }
    let db = StrategicDb::connect(config).await?;
    let quest = strategic_core::Quest {
        id: args.id,
        title: args.title,
        description: args.description,
        status: strategic_core::QuestStatus::Available,
        reward: args.reward,
    };
    db.upsert_quest(&quest)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!(
        "Upserted quest {} via Spacetime endpoint {}",
        quest.id, args.database_url
    );
    Ok(())
}
