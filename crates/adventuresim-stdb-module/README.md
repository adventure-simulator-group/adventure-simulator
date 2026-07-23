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
- ❌ Tactical tick-by-tick damage/combat state (strategic autoresolve stores final wounds, spent ammunition, and a diagnostic report rather than live combat state)
- ❌ Loot drops

Tactical state lives in the `tactical-server` game state and is discarded when missions end.

## Tables (9)

1. `player` - SpacetimeDB identity → character mapping
2. `character` - Character progression (XP, level)
3. `inventory_item` - Persistent items
4. `party` - Party groups
5. `party_member` - Party membership
6. private `mission_authority` - Tactical-to-strategic mission binding
7. private `case_authority` / `case_outcome_fact` - Objective authority and idempotent facts
8. private `contract_authority` - Offered, accepted, reportable, and paid agreements
9. private `case_custody` - Unique versioned asset and subject custody

## Key Reducers

- `upsert_character(id, name)` - Create/update character
- `create_party(id, name, leader_id)` - Create party
- `join_party(party_id, character_id)` - Join party
- `request_tactical_server(character_id, mission_id)` - Request a bound mission
- `autoresolve_mission(character_id, mission_id)` - Commit a trusted strategic battle result
- `accept_contract(character_id, contract_id)` - Accept an existing case's contract
- `abandon_contract(character_id, contract_id)` - Withdraw without deleting the case
- `report_contract(character_id, contract_id)` - Report a resolved case and pay exactly once

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
let items = vec![("lubeck_mark", 10), ("health_potion", 2)];
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
