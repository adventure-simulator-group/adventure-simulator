# Adventure Simulator · Bevy + SpacetimeDB demo (web)

This demo runs **Bevy in the browser** (WASM) and connects directly to **SpacetimeDB** (public network) using the JS SDK as the transport layer.

## Build

From the repo root:

`./utils/build_spacetimedb_bevy_demo.sh --release`

## Run

Serve this directory with any static file server. Example:

`python3 -m http.server -d ui/spacetimedb_bevy_demo 8000`

Then open:

`http://127.0.0.1:8000/`

Controls: `WASD` move · `E` interact · `R` respawn
