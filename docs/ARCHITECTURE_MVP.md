# Architecture MVP - Adventure Simulator

This is a fast Minimum Viable Product client server architecture for Adventure Simulator.
No edgegap deployments, no multi-cloud. All running on just one VPS.
- **Strategic Layer**: Web-based overworld UI for party management and mission selection
- **Tactical Layer**: Instanced Bevy/Lightyear game servers for real-time combat

## Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Browser (WASM)                           │
│  ┌─────────────────────┐    ┌─────────────────────────────────┐ │
│  │   Strategic Map UI  │    │   Tactical Client (Bevy/WASM)   │ │
│  │   (HTML/JS)         │    │   - WebTransport connection     │ │
│  └──────────┬──────────┘    │   - Renders GLB scene           │ │
│             │               │   - Player input/combat         │ │
│             │               └───────────────┬─────────────────┘ │
└─────────────┼───────────────────────────────┼───────────────────┘
              │ HTTP                          │ WebTransport
              ▼                               ▼
┌─────────────────────────────┐   ┌──────────────────────────────┐
│     strategic-api           │   │     tactical-server          │
│     (Axum HTTP Server)      │   │     (Bevy Headless Server)   │
│                             │   │                              │
│  - Party/Character CRUD     │   │  - Lightyear authoritative   │
│  - Mission orchestration    │   │  - GLB spawn point parsing   │
│  - Spawns tactical servers  │──▶│  - Token validation (HMAC)   │
│  - Persists results to DB   │◀──│  - Mission timeout + commit  │
│                             │   │                              │
└──────────────┬──────────────┘   └──────────────────────────────┘
               │                              │
               ▼                              │ POST /api/mission/commit
┌─────────────────────────────┐               │
│        PostgreSQL           │◀──────────────┘
│  - characters               │
│  - parties, party_members   │
│  - missions, mission_commits│
│  - inventory, quests        │
└─────────────────────────────┘
```

## Key Components

### 1. strategic-api (`crates/strategic/strategic-api`)

The strategic-api is an Axum HTTP server that:

- **Serves the Strategic Map UI** at `/` (HTML page with Town A/B buttons)
- **Manages parties and characters** via REST endpoints
- **Orchestrates tactical missions** by:
  1. Validating the `scene_key` against an allowlist
  2. Allocating a free port (6000-6100)
  3. Generating an HMAC-signed join token
  4. Spawning `tactical-server` as a child process
  5. Recording the mission in the database
- **Persists mission results** via idempotent commit

#### API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | Strategic map HTML page |
| `/health` | GET | Health check |
| `/api/party/create` | POST | Create a new party |
| `/api/party/join` | POST | Join an existing party |
| `/api/party/{id}` | GET | Get party details |
| `/api/mission/start` | POST | Start a tactical mission |
| `/api/mission/commit` | POST | Commit mission results (idempotent) |
| `/api/me/state` | GET | Get current player state |
| `/api/characters/{id}/state` | GET | Get character state with inventory |
| `/api/characters` | POST | Create/update a character |

#### Scene Allowlist

The strategic-api maintains a hardcoded allowlist of valid scenes:

```rust
fn get_scene_allowlist() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert("town_a", "assets/TownA.glb");
    map.insert("town_b", "assets/TownB.glb");
    map
}
```

**Security Note**: Client-provided `scene_key` values are validated against this allowlist. Arbitrary file paths are NEVER accepted.

### 2. tactical-server (`crates/tactical-server`)

The tactical-server is a headless Bevy application that:

- **Runs as a separate OS process** (spawned by strategic-api)
- **Uses Lightyear with WebTransport** for browser connectivity
- **Parses GLB files** to extract spawn markers (headless, via `gltf` crate)
- **Validates join tokens** using HMAC-SHA256 shared secret
- **Auto-terminates** after 60 seconds and commits results to strategic-api

#### Command-Line Arguments

```bash
tactical-server \
  --port 6000 \
  --mission-id "uuid-here" \
  --scene-key "town_a" \
  --asset-path "assets/TownA.glb" \
  --strategic-api-url "http://127.0.0.1:8080" \
  --hmac-secret "shared-secret"
```

### 3. GLB Spawn Marker Convention

Spawn points are defined in GLB/GLTF files using node naming conventions:

| Prefix | Type | Description |
|--------|------|-------------|
| `spawn_player` / `spawn_player_*` | Player | Player spawn point(s) |
| `spawn_enemy` / `spawn_enemy_*` | Enemy | Enemy spawn point(s) |
| `spawn_item` / `spawn_item_*` | Item | Item pickup location(s) |
| `exit` / `exit_*` | Exit | Mission exit point(s) |

Alternative: Use GLTF "extras" JSON on nodes:
```json
{ "spawn_type": "enemy", "enemy_type": "goblin" }
```

Example marker layout in `TownA.glb`:
```
spawn_player       at [0, 0, 0]
spawn_enemy_goblin_1 at [5, 0, 5]
spawn_enemy_goblin_2 at [-5, 0, 5]
spawn_enemy_goblin_3 at [0, 0, 10]
spawn_item_chest    at [3, 0, 8]
exit_north         at [0, 0, 15]
```

### 4. Database Schema

The strategic-api uses PostgreSQL with the following tables:

```sql
-- Core character state
CREATE TABLE characters (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    hp_current INT NOT NULL,
    hp_max INT NOT NULL,
    alive BOOL NOT NULL,
    deaths INT NOT NULL DEFAULT 0,
    xp INT NOT NULL DEFAULT 0,
    respawn_at_ms BIGINT NULL,
    updated_at_ms BIGINT NOT NULL
);

-- Inventory system
CREATE TABLE inventory_items (
    character_id TEXT NOT NULL REFERENCES characters(id),
    item_id TEXT NOT NULL,
    qty INT NOT NULL,
    PRIMARY KEY (character_id, item_id)
);

-- Party system
CREATE TABLE parties (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    leader_id TEXT NOT NULL REFERENCES characters(id),
    created_at_ms BIGINT NOT NULL
);

CREATE TABLE party_members (
    party_id TEXT NOT NULL REFERENCES parties(id),
    character_id TEXT NOT NULL REFERENCES characters(id),
    joined_at_ms BIGINT NOT NULL,
    PRIMARY KEY (party_id, character_id)
);

-- Mission tracking
CREATE TABLE missions (
    id TEXT PRIMARY KEY,
    party_id TEXT NOT NULL REFERENCES parties(id),
    scene_key TEXT NOT NULL,
    status TEXT NOT NULL,  -- 'active', 'completed', 'failed', 'cancelled'
    server_port INT NOT NULL,
    join_token TEXT NOT NULL,
    created_at_ms BIGINT NOT NULL,
    completed_at_ms BIGINT NULL
);

-- Idempotent commit tracking
CREATE TABLE mission_commits (
    mission_id TEXT PRIMARY KEY REFERENCES missions(id),
    committed_at_ms BIGINT NOT NULL,
    xp_gained INT NOT NULL,
    items_gained JSONB NOT NULL
);
```

### 5. Token Authentication

Join tokens use HMAC-SHA256 for authentication:

**Token Format**: `{mission_id}:{party_id}:{signature}`

**Generation** (strategic-api):
```rust
let payload = format!("{}:{}", mission_id, party_id);
let signature = hmac_sha256(hmac_secret, payload);
let token = format!("{}:{}", payload, base64(signature));
```

**Validation** (tactical-server):
```rust
fn validate_join_token(token: &str, hmac_secret: &str) -> Option<(MissionId, PartyId)>
```

## Running the MVP

### Prerequisites

1. **PostgreSQL** running with a `strategic` database:
   ```bash
   docker run -d --name strategic-db \
     -e POSTGRES_PASSWORD=postgres \
     -p 5432:5432 postgres:15

   docker exec -it strategic-db createdb -U postgres strategic
   ```

2. **Rust toolchain** (latest stable)

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://postgres:postgres@localhost:5432/strategic` | PostgreSQL connection |
| `HMAC_SECRET` | `dev-secret-change-in-production` | Shared secret for tokens |
| `STRATEGIC_API_URL` | `http://127.0.0.1:8080` | URL for tactical->strategic callbacks |

### Start the Strategic API

```bash
cd adventure-simulator-1
cargo run --bin strategic-api
```

The server starts at `http://localhost:8080`.

### Demo Flow

1. Open `http://localhost:8080` in a browser
2. Enter a Character ID and Name, click "Create Character"
3. Enter a Party Name, click "Create Party"
4. Click "Town A" or "Town B" to start a mission
5. The tactical-server process spawns automatically
6. Click "Simulate Victory" or "Simulate Defeat" to commit results
7. Observe XP and inventory updates in the Player State panel

## Design Seams for Future Scaling

### Replacing Local Process Spawn with Edgegap

The `start_mission` function currently spawns processes locally:

```rust
tokio::process::Command::new("cargo")
    .args(["run", "--bin", "tactical-server", ...])
    .spawn()
```

To integrate with Edgegap:
1. Replace with HTTP call to Edgegap API to request a deployment
2. Edgegap returns connection info (host, port, certificate)
3. Return this info in `StartMissionResponse`

The API contract remains unchanged:
```json
{
  "mission_id": "...",
  "server_addr": "edge-server.edgegap.net",
  "server_port": 7777,
  "join_token": "...",
  "certificate_digest": "..."
}
```

### Adding More Scenes

1. Create a new GLB file with spawn markers (e.g., `Dungeon.glb`)
2. Add to the allowlist in `strategic-api/src/main.rs`:
   ```rust
   map.insert("dungeon", "assets/Dungeon.glb");
   ```
3. Add a button/location in `map.html`

### Implementing Full Combat

The tactical-server MVP logs spawn markers but doesn't run combat. To add:

1. Initialize Lightyear server in `setup_server`
2. Spawn player entities at `SpawnMarkerType::Player` positions
3. Spawn enemy entities at `SpawnMarkerType::Enemy` positions
4. Implement combat systems (damage, death, loot)
5. Track kills in `MissionState`
6. End mission when all enemies killed or players exit

## Constraints Observed

- **No DB calls during gameplay tick**: Only at mission start/end
- **Everything in Rust**: No external scripting
- **Scene allowlist**: Never accept arbitrary paths from clients
- **Idempotent commit**: `mission_commits` table prevents double-counting rewards
