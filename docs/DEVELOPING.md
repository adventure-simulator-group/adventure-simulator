# Development Workflow

Local workflow for running the Adventure Simulator demo.

## Architecture Overview

The game uses a Mount & Blade style architecture:

1. **Strategic Layer** (SpacetimeDB) - Character progression, inventory, mission tracking
2. **Tactical Spawner** - Watches for pending missions, spawns tactical servers
3. **Tactical Server** (Lightyear) - Real-time 3D gameplay server
4. **Browser Client** (WASM/Bevy) - Connects to tactical server via WebTransport

**Flow:**
1. Player clicks a location in the browser → SpacetimeDB creates a "pending" mission
2. Tactical spawner sees pending mission → starts a adventuresim-tactical-server process
3. Tactical server writes connection info to SpacetimeDB → status becomes "ready"
4. Browser polls and sees "ready" → loads WASM game and connects to tactical server
5. Mission ends → tactical server commits results and exits

## Quick Start

```bash
just dev
```

Open http://localhost:8000/map.html

## Full Development (with Tactical Servers)

To run the complete stack with automatic tactical server spawning:

**Terminal 1:** Start SpacetimeDB and UI
```bash
just dev
```

**Terminal 2:** Run the tactical spawner (watches for missions)
```bash
just spawner
```

**Terminal 3:** (Optional) Build WASM client
```bash
just build-wasm
```

Now when you click a location in the browser, a tactical server will automatically spawn.

## Requirements

- Rust toolchain
- just (`cargo install just`)
- SpacetimeDB CLI (`curl -sSf https://install.spacetimedb.com | bash`)
- Python 3
- wasm-bindgen (`cargo install wasm-bindgen-cli`) - for WASM builds

## Services and Ports

| Service | Port | Description |
|---------|------|-------------|
| SpacetimeDB | 3000 | Strategic database |
| Strategic UI | 8000 | Browser UI (`map.html`) |
| Tactical Server | 6000+ | Game server (one per mission) |

## Core Commands

```bash
# Development
just dev              # Start SpacetimeDB + UI server
just web-damaged      # Start a fresh stack with an injured demo character
just spawner          # Run tactical server spawner
just build-wasm       # Build WASM client

# Testing
just tactical         # Run a single tactical server (for testing)
just status           # Check service status
just stop             # Stop all services

# Building
just build-tactical   # Build adventuresim-tactical-server and adventuresim-tactical-server-dispatcher
just build-all        # Build everything

# Database
just publish          # Publish SpacetimeDB module
just publish-reset    # Publish and clear database
```

## Strategic UI

The UI is served from `crates/adventuresim-stdb-module/static/`.
`map.html` connects to SpacetimeDB at `http://localhost:3000`.

## Tactical Spawner

The spawner polls SpacetimeDB for "pending" missions and starts adventuresim-tactical-server processes:

```bash
just spawner
```

Each tactical server:
1. Starts on an available port
2. Calls `tactical_server_ready` reducer with connection info
3. Runs for 2 minutes (or until mission ends)
4. Calls `commit_mission` reducer with results
5. Exits

## Testing a Single Server

For testing without the spawner:

```bash
just tactical mission_id="test-123" scene_key="town_a"
```

## Troubleshooting

- **SpacetimeDB not running:** `just status`, then `just spacetime-start`
- **SpacetimeDB failed to start:** check `/tmp/adventure-simulator-1/spacetime.log`
- **Tactical spawner can't find binary:** run `just build-tactical` first
- **Mission stuck on "pending":** spawner not running or binary not found
