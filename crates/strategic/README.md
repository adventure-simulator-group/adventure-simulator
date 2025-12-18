# Strategic layer (Postgres + HTTP + SSE)

This folder is an early, intentionally small “strategic layer” scaffold:

- `strategic-core`: shared types (quests, characters, inventory, loot).
- `strategic-db`: Postgres schema + transactional DB operations.
- `strategic-server`: axum HTTP server (JSON API + Datastar-friendly HTML + SSE).
- `strategic-plugin`: Bevy-friendly HTTP client wrapper (WIP).
- `strategic-cli`: helper CLI to upsert quests into Postgres.

## Quick start

1) Start Postgres (Docker):

`docker run -d --name strategic-db -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:15`

Create the database once:

`docker exec -it strategic-db createdb -U postgres strategic`

2) Run the strategic server:

`DATABASE_URL=postgres://postgres:postgres@localhost:5432/strategic cargo run -p strategic-server`

3) Open the Datastar overlay (served by `strategic-server`):

`http://127.0.0.1:8080/overlay/`

4) Run the playable demo (spawns a player, quest objects, and a hazard bot):

`cargo run -p adventure-simulator-demo`
