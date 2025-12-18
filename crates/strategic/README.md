# Strategic layer (SpacetimeDB module)

This branch replaces the old Postgres + HTTP/SSE strategic demo with a **SpacetimeDB module**:

- `strategic-stdb-module`: authoritative strategic + world-state tables, reducers, and a scheduled `world_tick` loop.
- `ui/spacetimedb_demo`: a static browser demo (canvas world + overlay) that connects directly to SpacetimeDB on the public network.
- `ui/spacetimedb_bevy_demo`: a Bevy-in-the-browser demo (WASM) that connects directly to SpacetimeDB (JS SDK transport + wasm-bindgen bridge).

## Publish (public network)

1) Install the SpacetimeDB CLI (`spacetime`):

`https://spacetimedb.com/install`

2) Login:

`spacetime login`

3) Publish the module (from the module directory):

`cd crates/strategic/strategic-stdb-module && spacetime publish adventure-simulator-strategic-demo`

Copy the database identity printed by `spacetime publish` (you’ll paste it into the browser demo).

## Run the browser demo

Serve `ui/spacetimedb_demo` with any static file server. Example:

`python3 -m http.server -d ui/spacetimedb_demo 8000`

Then open:

`http://127.0.0.1:8000/`

In the overlay:
- `Host`: defaults to `https://maincloud.spacetimedb.com` (change if you published elsewhere)
- `DB`: paste the database identity/name from `spacetime publish`
- Press `Connect`

Controls: `WASD` move · `E` interact · `R` respawn

## Run the Bevy (WASM) browser demo

1) Build the wasm-bindgen bundle:

`./utils/build_spacetimedb_bevy_demo.sh --release`

(If the script reports a wasm-bindgen version mismatch, install the requested `wasm-bindgen-cli` version.)

2) Serve `ui/spacetimedb_bevy_demo` with any static file server. Example:

`python3 -m http.server -d ui/spacetimedb_bevy_demo 8001`

Then open:

`http://127.0.0.1:8001/`

In the overlay:
- `Host`: defaults to `https://maincloud.spacetimedb.com`
- `DB`: paste the database identity/name from `spacetime publish`
- Press `Connect`

Controls: `WASD` move · `E` interact · `R` respawn
