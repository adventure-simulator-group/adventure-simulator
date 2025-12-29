# Architecture MVP - Adventure Simulator

Minimal SpacetimeDB implementation for Adventure Simulator.

- **Strategic Layer**: SpacetimeDB for character progression, inventory, parties, missions
- **Tactical Layer**: Bevy/Lightyear game servers for real-time combat (in-memory state only)

## Key Principle

**Tactical gameplay state (HP, damage, positions, enemies, loot drops) lives ONLY in the tactical-server game state.**

The database stores ONLY:
- Character progression (XP, level)
- Persistent inventory
- Party membership
- Mission tracking
- Quest progress

When a mission ends, the tactical server sends the **results** (XP gained, items earned) to SpacetimeDB via the `commit_mission` reducer.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Browser                                  │
│  ┌─────────────────────┐    ┌─────────────────────────────────┐ │
│  │   Strategic Map UI  │    │   Tactical Client (Bevy/WASM)   │ │
│  │   (HTML/JS)         │    │   - WebTransport connection     │ │
│  └──────────┬──────────┘    │   - Renders GLB scene           │ │
│             │               │   - HP/damage in game state     │ │
│             │               └───────────────┬─────────────────┘ │
└─────────────┼───────────────────────────────┼───────────────────┘
              │ WebSocket                     │ WebTransport
              ▼                               ▼
┌─────────────────────────────┐   ┌──────────────────────────────┐
│      SpacetimeDB            │   │     tactical-server          │
│   strategic-stdb-module     │   │     (Bevy Headless Server)   │
│                             │   │                              │
│  - Character progression    │   │  - Lightyear authoritative   │
│  - Persistent inventory     │   │  - GLB spawn point parsing   │
│  - Parties & missions       │   │  - ALL tactical state (HP,   │
│  - Quest tracking           │   │    damage, positions) in     │
│                             │   │    game state ONLY           │
└─────────────────────────────┘   └──────────────────────────────┘
              ▲                              │
              │  commit_mission(xp, items)   │
              └──────────────────────────────┘
```

## SpacetimeDB Module

### Tables (9 total)

| Table | Description |
|-------|-------------|
| `player` | Maps SpacetimeDB identity to character |
| `character` | Character progression (XP, level) - NO HP/damage! |
| `inventory_item` | Persistent items |
| `party` | Party groups |
| `party_member` | Party membership |
| `mission` | Active and completed missions |
| `mission_commit` | Idempotent mission result tracking |
| `quest_def` | Quest definitions |
| `character_quest` | Per-character quest progress |
| `port_allocation` | Tactical server port allocation (singleton) |

### Key Reducers

| Reducer | Description |
|---------|-------------|
| `upsert_character` | Create/update character (gives starter items) |
| `add_item_to_inventory` | Add items |
| `create_party` / `join_party` / `leave_party` | Party management |
| `start_mission` | Allocate port, record mission |
| **`commit_mission`** | **Apply mission results (XP, items) - idempotent** |
| `cancel_mission` | Cancel active mission |
| `start_quest` / `complete_quest` | Quest management |

## tactical-server

The tactical server is a headless Bevy application that:

- Runs as a separate OS process
- Uses Lightyear with WebTransport
- Parses GLB files for spawn markers
- **Maintains ALL tactical state in game memory** (HP, damage, positions, enemies, loot)
- **Commits only the final results** to SpacetimeDB when the mission ends

### Command-Line Arguments

```bash
tactical-server \
  --port 6000 \
  --mission-id "mission-123" \
  --scene-key "town_a" \
  --asset-path "assets/TownA.glb" \
  --spacetimedb-url "http://localhost:3000" \
  --spacetimedb-module "strategic-stdb-module" \
  --hmac-secret "shared-secret"
```

### Mission End Flow

When the tactical mission ends (timeout, victory, or defeat):

1. Tactical server computes final results in memory:
   - Total XP gained (based on enemies defeated, objectives completed)
   - Items earned (loot from enemies, quest rewards)
2. Tactical server calls SpacetimeDB `commit_mission` reducer:
   ```rust
   commit_mission(mission_id, success, xp_gained, items_gained_json)
   ```
3. SpacetimeDB applies rewards to all party members
4. Tactical server terminates

## Running the MVP

### 1. Install SpacetimeDB CLI

```bash
curl -sSL https://install.spacetimedb.com | sh
```

### 2. Start SpacetimeDB

```bash
spacetime start
```

### 3. Publish the Module

```bash
cd crates/strategic-server/strategic-stdb-module
spacetime publish strategic-stdb-module
```

### 4. Open the UI

Open `crates/strategic-server/strategic-stdb-module/static/map.html` in a browser

### 5. Demo Flow

1. Enter a Character ID and Name, click "Create Character"
2. Enter a Party Name, click "Create Party"
3. Click "Town A" or "Town B" to start a mission
4. Click "Simulate Victory" or "Simulate Defeat"
5. Observe XP and items update in real-time

## Scene Allowlist

Scenes are validated against a hardcoded allowlist:

```rust
const VALID_SCENES: &[&str] = &["town_a", "town_b"];
```

**Security**: Client-provided `scene_key` values are validated. Arbitrary values are rejected.

## GLB Spawn Marker Convention

Spawn points are defined in GLB/GLTF files using node naming:

| Prefix | Type | Description |
|--------|------|-------------|
| `spawn_player` / `spawn_player_*` | Player | Player spawn point(s) |
| `spawn_enemy` / `spawn_enemy_*` | Enemy | Enemy spawn point(s) |
| `spawn_item` / `spawn_item_*` | Item | Item pickup location(s) |
| `exit` / `exit_*` | Exit | Mission exit point(s) |

## Benefits of This Architecture

1. **Simplicity**: Single database technology (SpacetimeDB only)
2. **Performance**: No DB calls during tactical gameplay
3. **Correctness**: Tactical game state lives where it belongs (in-memory)
4. **Idempotency**: `commit_mission` can be called multiple times safely
5. **Real-time**: Strategic UI gets instant updates via SpacetimeDB subscriptions

## Constraints

- **No DB calls during gameplay tick**: Only at mission start/end
- **Everything in Rust**: No external scripting
- **Scene allowlist**: Never accept arbitrary paths from clients
- **Idempotent commit**: Prevents double-counting rewards
- **Tactical state is ephemeral**: HP/damage/positions disappear when mission ends
