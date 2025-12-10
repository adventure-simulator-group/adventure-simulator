# Strategic demo stack

Minimal scaffolding for the strategic layer demo:

- `strategic-core`: shared types (quests, statuses).
- `strategic-db`: Postgres access + schema/seed helpers (demo quests via app seeds, e.g., `quest.pet_cat`).
- `strategic-server`: axum HTTP server exposing `/health`, `/quests/:id`, `/quests/:id/complete`.
- `strategic-plugin`: Bevy-friendly HTTP client wrapper for the strategic API.
- `strategic-cli`: simple seeding CLI.

Quick start:
1. Start Postgres (example): `docker run -d --name strategic-db -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:15`
2. Seed (example quest via CLI or app-specific seeder): `cargo run -p strategic-cli -- --database-url postgres://postgres:postgres@localhost:5432/strategic --id quest.pet_cat --title "Pet the Cat" --description "Say hello and pet the cat."`
3. Serve: `cargo run -p strategic-server`
4. From the tactical/lightyear side, POST to `/quests/{id}/complete` to mark a quest complete.
