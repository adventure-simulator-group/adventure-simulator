# Architecture MVP - Adventure Simulator

Minimal SpacetimeDB implementation for Adventure Simulator.

- **Strategic Layer**: SpacetimeDB for character progression, inventory, parties, missions
- **Tactical Layer**: Bevy/Lightyear game servers for real-time combat (in-memory state only)

## Key Principle

**Tactical gameplay state (HP, damage, positions, enemies, loot drops) lives ONLY in the adventuresim-tactical-server game state.**

## Strategic browser updates

The strategic browser is server-authoritative. Browsers submit discrete commands
to `strategic-web` over authenticated HTTP and never connect to SpacetimeDB
directly. `strategic-web` owns a single generated-client WebSocket subscription
to SpacetimeDB and fans database changes out to authenticated pages as Datastar
server-sent events.

Browser-facing deployments terminate HTTPS at a reverse proxy and negotiate
HTTP/2 or HTTP/3. Multiplexing prevents the long-lived Datastar SSE stream from
consuming one of the browser's small HTTP/1.1 per-origin connection pool while
navigation and component refresh requests are pending. The Rust web process
remains an internal HTTP service; local development uses `Caddyfile.dev` to
expose it at `https://localhost:8443`.

The SSE stream patches a stable, server-rendered revision marker. Strategic UI
components subscribe to that marker and refresh only their relevant state. This
drives canonical-location navigation, party portraits, party requests and
notifications, recruitment roles and applicants, inventories and loot, map and
quest rails, selected-character details, the active quest indicator, service
quest badges and conversations, mission readiness, incoming
local-chat portraits, and local conversation history. Shared page regions are
refetched from their canonical server-rendered URL and replaced only when their
markup changes. A region with a staged inventory operation, focused control, or
open role editor is deliberately left untouched until that local interaction is
finished. The stream's initial revision establishes a baseline and does not
refetch the page that was just rendered. Later revisions are coalesced, and the
client checks canonical navigation before refreshing regions so travel does not
refetch the location being left. New strategic live UI should extend this stream
with stable Maud fragment roots rather than add polling timers or expose module
credentials to browsers.

Background component GETs are keyed and replace older requests for the same
region. They are canceled when navigation begins or the page is discarded, and
initial component hydration is serialized to avoid a request burst before the
live stream is established. Hidden pages use Datastar's default behavior of
closing the SSE stream and reopening it when visible.

Displayed strategic time is the exception to SSE invalidation. The browser
fetches one character/official-time snapshot when a page initializes, then
advances both clocks locally at the configured game-time ratio. SpacetimeDB
derives authoritative time from the stored epoch when an action needs it; it
does not write a world-clock row every second.

The database stores ONLY:
- Character progression (XP, level)
- Persistent inventory
- Party membership
- Mission tracking
- Quest progress

When a mission ends, the tactical server sends the **results** (XP gained, items earned) to SpacetimeDB via the `commit_mission` reducer.

Finalized loot is strategic state. The tactical server derives drops from the temporary enemies' equipped inventory and records only the resulting item identifiers and quantities. The strategic layer owns the post-battle result, shared party inventory, and per-character value stakes; no enemy, damage, position, or other tactical tick state is persisted.

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
│      SpacetimeDB            │   │     adventuresim-tactical-server          │
│   adventuresim-stdb-module     │   │     (Bevy Headless Server)   │
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

### Strategic tables

| Table | Description |
|-------|-------------|
| `player` | Maps SpacetimeDB identity to character |
| `character` | Character progression (XP, level) - NO HP/damage! |
| `inventory_item` | Persistent items |
| `party` | Party groups, active quest, and aggregate skill-check targets; every character belongs to at least a solo party |
| `party_member` | Party membership, including the recruitment role that filled a slot |
| `party_recruitment_role` | Named party-independent role requirements and slot quantities |
| `saved_recruitment_role` | Reusable named role requirement presets owned by a character |
| `party_join_request` | Pending applications to a recruitment role |
| `party_action_request` / `party_leader_vote` | Persistent member suggestions and dead-leader succession votes |
| `local_chat_message` | Party-scoped NPC and face-to-face player conversation history |
| `character_capability` | Cached automatic equipment, attribute, skill, and mobility tags |
| `mission` | Active and completed missions |
| `mission_commit` | Idempotent mission result tracking |
| `quest` | Settlement-owned generated postings, off-road locations, and acceptance state |
| `character` / `party` location fields | Current settlement or quest location (never tactical positions) |
| `port_allocation` | Tactical server port allocation (singleton) |

### Key Reducers

| Reducer | Description |
|---------|-------------|
| `upsert_character` | Create/update character (gives starter items) |
| `add_item_to_inventory` | Add items |
| `backfill_solo_parties` / `leave_party` / `disband_party` | Maintain the invariant that every character has a party |
| `create_recruitment_role` / `update_recruitment_role` / `delete_recruitment_role` | Create, resize, edit, and remove grouped party recruitment slots |
| `save_recruitment_role` / `delete_saved_recruitment_role` | Manage reusable role presets |
| `update_party_check_targets` | Configure non-filtering Medicine, Surgery, Charisma, and Faith aggregate goals |
| `request_to_join_party` / `accept_party_join_request` / `reject_party_join_request` | Role recruitment and atomic party merging; destination leadership remains intact while source members, pooled assets, and stakes transfer |
| `request_general_party_join` | Submit a retained application through a shared zero-capacity Unassigned role |
| `send_local_chat_message` / `record_local_npc_message` | Persist location-gated, party-owned Local conversations |
| `refresh_capabilities` | Recompute automatic character tags through the shared core evaluator |
| `ensure_settlement_activity` | Maintain 3–5 visible quests and 1–2 locally generated recruiting NPC quest parties |
| `start_mission` | Allocate port, record mission |
| **`commit_mission`** | **Apply mission results (XP, items) - idempotent** |
| `cancel_mission` | Cancel active mission |
| `start_quest` / `complete_quest` | Quest management |
| `travel_to_quest` | Advance strategic time and move a party to its off-road quest location |
| `autoresolve_quest` | Apply a placeholder victory, rewards, and final persistent injury results |

## adventuresim-tactical-server

The tactical server is a headless Bevy application that:

- Runs as a separate OS process
- Uses Lightyear with WebTransport
- Parses GLB files for spawn markers
- **Maintains ALL tactical state in game memory** (HP, damage, positions, enemies, loot)
- **Commits only the final results** to SpacetimeDB when the mission ends

### Command-Line Arguments

```bash
adventuresim-tactical-server \
  --port 6000 \
  --mission-id "mission-123" \
  --scene-key "town_a" \
  --asset-path "assets/TownA.glb" \
  --spacetimedb-url "http://localhost:3000" \
  --spacetimedb-module "adventuresim-stdb-module" \
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
cd crates/adventuresim-stdb-module
spacetime publish adventuresim-stdb-module
```

### 4. Open the UI

Run `just web`, then open `http://localhost:8080` in a browser.

### 5. Demo Flow

1. Enter a Character ID and Name, click "Create Character"
2. Use the plus button to the right of the filled party portraits to add recruitment roles and slots
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
- **Quest locations are strategic places**: their identity and travel coordinates persist, but no enemies, tactical positions, or combat ticks are stored there. Autoresolve writes only final injury and reward results.
