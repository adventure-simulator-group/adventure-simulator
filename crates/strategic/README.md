# Strategic layer (SpacetimeDB + HTTP + SSE)

This folder is an early, intentionally small “strategic layer” scaffold:

- `strategic-core`: shared types (quests, characters, inventory, loot).
- `strategic-db`: lightweight facade over an in-memory store with optional SpacetimeDB event publishing.
- `strategic-server`: axum HTTP server (JSON API + Datastar-friendly HTML + SSE) that can emit SpacetimeDB mutations.
- `strategic-plugin`: Bevy-friendly HTTP client wrapper (WIP).
- `strategic-cli`: helper CLI to upsert quests into SpacetimeDB.

## Quick start

1) Obtain a SpacetimeDB mutation endpoint (cloud console) and API key (optional for public modules).
2) Export the endpoint (and optionally the API key) and run the strategic server:

`SPACETIME_ENDPOINT=https://your-module-endpoint cargo run -p strategic-server`

3) Open the Datastar overlay (served by `strategic-server`):

`http://127.0.0.1:8080/overlay/`

4) Run the playable demo (spawns a player, quest objects, and a hazard bot):

`cargo run -p adventure-simulator-demo`
