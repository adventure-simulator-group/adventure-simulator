# SpacetimeDB browser demo

This folder is a **static** browser demo (no build step) that connects directly to a published SpacetimeDB database:

- A simple canvas “world” view (players, hazard bot, pickups, loot bags)
- A debug overlay for connection + character state

It’s intended to be embedded as an overlay on top of a WASM game canvas later.

## Run

Serve this folder with any static file server, e.g.:

`python3 -m http.server -d ui/spacetimedb_demo 8000`

Open:

`http://127.0.0.1:8000/`

## Connect

- `Host`: defaults to `https://maincloud.spacetimedb.com`
- `DB`: the database name/identity printed by `spacetime publish`
- `Character` / `Name`: arbitrary strings for the demo

Controls: `WASD` move · `E` interact · `R` respawn

## Notes

- The client uses `@clockworklabs/spacetimedb-sdk` via `esm.sh`.
- The module schema is currently mirrored manually in `ui/spacetimedb_demo/spacetimedb_demo.js`. If you change table/reducer shapes in `crates/strategic/strategic-stdb-module`, update the schema here too (or switch to codegen + bundling later).

