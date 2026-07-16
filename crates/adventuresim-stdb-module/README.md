# strategic-stdb-module

Minimal SpacetimeDB module for Adventure Simulator's strategic layer.

## What This Stores

**ONLY strategic (meta-game) state:**

- ✅ Character progression (XP, level)
- ✅ Persistent inventory
- ✅ Party membership
- ✅ Mission tracking
- ✅ Quest progress

**NOT tactical (gameplay) state:**

- ❌ HP (current/max)
- ❌ Alive/dead status
- ❌ Enemy positions
- ❌ Player positions
- ❌ Tactical tick-by-tick damage/combat state (strategic autoresolve stores only final wounds)
- ❌ Loot drops

Tactical state lives in the `tactical-server` game state and is discarded when missions end.

## Tables (9)

1. `player` - SpacetimeDB identity → character mapping
2. `character` - Character progression (XP, level)
3. `inventory_item` - Persistent items
4. `party` - Party groups
5. `party_member` - Party membership
6. `mission` - Mission records
7. `mission_commit` - Idempotent commit tracking
8. `quest_def` - Quest definitions
9. `character_quest` - Per-character quest progress

## Key Reducers

- `upsert_character(id, name)` - Create/update character
- `create_party(id, name, leader_id)` - Create party
- `join_party(party_id, character_id)` - Join party
- `start_mission(id, party_id, scene, token)` - Start mission
- **`commit_mission(id, success, xp, items_json)`** - **Commit mission results (idempotent)**
- `complete_quest(character_id, quest_id)` - Complete quest

## Publishing

Use the repository-pinned SpacetimeDB CLI 2.6.1. The pre-launch upgrade from
1.x is a deliberate reset/reseed and does not preserve existing database data.
After that reset, use plain publishing for normal updates; reset publishing
always deletes data and must not be used once player data needs preservation.

```bash
# Start SpacetimeDB
spacetime start

# Publish module
cd crates/strategic-server/strategic-stdb-module
spacetime publish strategic-stdb-module

# Start the server-rendered strategic UI from the workspace root
just web
```

## Usage

The strategic web UI demonstrates:

1. Creating a character (gets starter items)
2. Creating a party
3. Starting a mission (allocates port, records mission)
4. Committing mission results (applies XP and items to party members)

The tactical server calls `commit_mission` when the mission ends:

```rust
// Tactical server computes results in memory
let xp_gained = enemies_killed * 25;
let items = vec![("gold_coin", 10), ("health_potion", 2)];
let items_json = serde_json::to_string(&items)?;

// Commit to SpacetimeDB
spacetimedb_http_call(
    "http://localhost:3000",
    "strategic-stdb-module",
    "commit_mission",
    &[mission_id, success, xp_gained, items_json]
)?;
```

## Architecture

```
Browser HTML UI
    │ WebSocket
    ▼
SpacetimeDB (strategic-stdb-module)
    ▲
    │ commit_mission(xp, items)
    │
Tactical Server (in-memory game state)
```

Tactical gameplay (HP, damage, positions) happens entirely in the tactical server's memory.
Only the final results (XP, items) are persisted to SpacetimeDB.
