# Development Workflow

Local workflow for running the strategic UI in the browser with a local SpacetimeDB instance.
The tactical server is optional and runs as a separate process.

## Quick Start

```bash
just dev
```

Open http://localhost:8000/map.html (or run `just dev-open` to open automatically).

## Full Development (with WASM Game)

To build and run the complete stack including the playable WASM tactical game:

```bash
just dev-full
```

This builds the WASM client, starts SpacetimeDB, publishes the module, and serves the UI.
After creating a character and party, click a mission location, then click **"Enter World"** to play the 3D game in your browser.

**Controls in-game:**
- WASD - Move
- Arrow Keys - Rotate camera
- ESC - Exit to map

## Requirements

- Rust toolchain (cargo)
- just (`cargo install just`)
- SpacetimeDB CLI (`curl -sSf https://install.spacetimedb.com | bash`)
- Python 3
- wasm-bindgen (`cargo install wasm-bindgen-cli`) - for WASM builds
- OpenSSL (only needed for WebTransport certificate generation)

## Services and Ports

- SpacetimeDB: http://localhost:3000
- Strategic UI: http://localhost:8000/map.html (`crates/strategic-server/strategic-stdb-module/static/map.html`)
- Tactical server: localhost:6000 (WebTransport)

## Core Commands

```bash
just dev              # Start SpacetimeDB + UI server
just dev-full         # Start everything + build WASM game
just dev-open         # Same as dev, but opens browser
just build-wasm       # Build WASM client only
just stop             # Stop all services
just status           # Check service status
just publish          # Publish SpacetimeDB module
just publish-reset    # Publish and clear database
just tactical         # Run tactical server (Town A)
just tactical-cert    # Run tactical server with certs
just tactical-town-a  # Run tactical server for Town A
just tactical-town-b  # Run tactical server for Town B
just certs            # Generate WebTransport certificates
```

## Strategic UI

The UI is served from `crates/strategic-server/strategic-stdb-module/static/`.
`map.html` connects to SpacetimeDB at `http://localhost:3000`.
If you need a different host or port, update `SPACETIME_HOST` in
`crates/strategic-server/strategic-stdb-module/static/map.html`.

You can also override at runtime with query params:

```
http://<host>:8000/map.html?spacetimedb=http://<host>:3000
```

By default, `just publish` targets the local SpacetimeDB instance (`http://localhost:3000`).
Override with `just publish server=https://maincloud.spacetimedb.com` if you need a remote publish.

## Tactical Server

Run a local mission (defaults to Town A):

```bash
just tactical
```

Run Town B instead:

```bash
just tactical scene_key="town_b" asset_path="assets/TownB.glb"
```

The tactical server runs for about 60 seconds, commits results to SpacetimeDB, then exits.

## WebTransport Certificates (browser clients)

When testing a browser-based tactical client, you need a certificate the browser will accept.

```bash
just certs
just tactical-cert
```

This generates `utils/cert.pem`, `utils/key.pem`, and `utils/digest.txt`, then runs the
server with those certs. Use the digest as the SHA-256 certificate hash for
`serverCertificateHashes` in your WebTransport client.

Native clients can use `just tactical` (self-signed cert in memory).

## Troubleshooting

- SpacetimeDB not running: `just status`, then `just spacetime-start`
- SpacetimeDB failed to start: check `/tmp/adventure-simulator-1/spacetime.log`
- UI not loading: ensure `just serve-ui` is running; check `/tmp/adventure-simulator-1/http.log`
- Ports in use: stop the other process or update the port variables in `justfile`

## VPS / Public Demo

This exposes SpacetimeDB and the UI to the public internet.
Make sure your firewall and DNS are configured before using it.

```bash
just vps-serve domain=adventure-sim-demo.xyz
```

You must open ports `3000` and `8000` on the VPS firewall and point your DNS
record to the VPS IP. For HTTPS domains, use a reverse proxy (Caddy/Nginx) so
both the UI and SpacetimeDB are served over HTTPS.
