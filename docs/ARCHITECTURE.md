# Architecture MVP - Adventure Simulator

Minimal SpacetimeDB implementation for Adventure Simulator.

- **Strategic Layer**: SpacetimeDB for character progression, inventory, parties, missions
- **Tactical Layer**: Bevy/Lightyear game servers for real-time combat (in-memory state only)

## Key Principle

**Tactical gameplay state (HP, damage, positions, enemies, loot drops) lives ONLY in the adventuresim-tactical-server game state.**

## Offline world compilation

Raw historical and geographic datasets are compiled outside SpacetimeDB by the
native `adventuresim-world-import` crate. Each upstream format has an isolated
source module; the outer builder combines them into a validated canonical
world. `adventuresim-world-schema` contains the dependency-light, versioned
import types shared by the compiler and strategic module. The strategic module
accepts those records through reducers but never parses raw datasets or depends
on native geospatial libraries.

Every gridded enrichment shares the canonical `SpatialGridSpec` described in
`docs/SPATIAL_GRID.md`. The complete spec and inference-rules version are
serialized in world metadata, so either changing alters the content-addressed
artifact identity. Source coverage extents remain source-manifest concerns.
The grid is compiler metadata, not a SpacetimeDB table shape: no grid columns or
tactical coordinates are persisted.

World schema v20 retains typed canonical
distribution manifests documented in `docs/SOURCE_MANIFESTS.md`. Their
schema/rules/year/grid/source digest is the cache and build boundary and is
retained by the import session alongside the complete artifact ID.

Source modules first parse into importer-only draft types. The outer builder
enriches that draft in dependency order and only then constructs the canonical
world schema. For example, Viabundus supplies settlement identity and road
topology, while GLO-30 supplies the required typed elevation for each draft
settlement. HYDE then adds an exhaustive typed land-use profile and constructs
an enriched draft. Copernicus forest cover consumes that draft and constructs
another enriched draft with a typed open-or-wooded state. Jung/IIASA European
PNV v1.1 then adds typed posterior, categorical, or inferred potential
vegetation. EU-Trees4F consumes that source-independent class accessor,
adds a nonempty modeled-or-inferred tree-species profile, and constructs another
enriched draft. SoilGrids consumes all of those environmental inputs and adds a
typed source-prediction draft. EGDI surface geology then uses that prediction and an
indexed local GeoPackage to attach a typed mapped-or-inferred lithology and age
setting. The IEG stage then parses a curated 1544 legal-religion
intermediate into an established, parity, multi-confessional, or municipally
determined typed status. NOAA OWDA adds a bounded current-summer PDSI and typed
twenty-year drought/wetness history. EU-Hydro adds settlement water access and
converts draft roads into typed land crossings or ferry waterways, returning a
private hydrology draft. The soil finalizer combines prediction, geology,
Jung wetland evidence, elevation, and hydrology but returns another private
draft. A final environmental-synthesis stage then consumes the entire evidence
chain and alone constructs canonical world records, including a reconstruction
of dominant 1544 cover distinct from modern-climate potential vegetation.
The terminal route-terrain stage then samples straight edge geometry from
GLO-30 and combines it with EU-Hydro route context into bounded strategic
profiles, landforms, risks, and encounter selectors. Those selectors are
coarse planning facts; they never persist tactical positions, HP, damage,
enemies, or ticks.
Rules-v6 industry inference then attaches a bounded strategic production
profile. Incident route accessibility may downgrade scale but never creates a
resource; the evidence model is documented in `docs/INDUSTRIES.md`.
Land-use sampled/normalized/fallback evidence remains private through this
stage so a deterministic missing-HYDE profile cannot masquerade as direct.
The generic draft is a typestate boundary: each enrichment
stage consumes only settlements that have all of its required predecessor data.
This keeps source-specific placeholders out of canonical records and prevents
later stages from being called before their dependencies exist.

The compiled world and persisted strategic tables retain source explanations as
bounded, unstructured Markdown in a `sources` field. The world-import session
stores the distribution-level list, while each imported node, travel edge, and
settlement stores record-specific notes describing direct samples,
interpolation, deterministic inference, and fallbacks. This is deliberately a
display/debug payload rather than a structured provenance API. No debug view
renders it yet; any future renderer must treat it as untrusted Markdown and
sanitize generated HTML.

Each compiled artifact is identified by a content hash over its serialized
metadata and records. An interrupted load may
resume only with the same artifact; a different artifact requires a database
reset. Successful loads explicitly complete their import session so later
batches cannot mutate an already-loaded world.

## Strategic browser updates

The strategic browser is server-authoritative. Browsers submit discrete commands
to `strategic-web` over authenticated HTTP and never connect to SpacetimeDB
directly. `strategic-web` owns a single generated-client WebSocket subscription
to the mutable tables that invalidate strategic UI fragments and fans those
database changes out to authenticated pages as Datastar server-sent events.
Large static world tables, including settlements, routes, aliases, and source
descriptions, stay out of this subscription and are queried on demand.

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
| `character` | Character progression, location, and life state; no tactical tick state |
| `character_limbs` | Final persistent body-part injury outcomes used by strategic recovery and checks |
| `character_condition` | Durable strategic blood volume, body weight, and religion selection |
| `settlement` | Strategic settlement data, including the single fixed faith represented by its current church |
| `morale_event` | Time-stamped strategic successes and setbacks with seven-day decay |
| `character_morale_source` | Refreshable named, signed contributions used by the morale meter breakdown |
| `character_strategic_condition` | Refreshable derived morale, ally-restoration percentage, and incapacitation projection for server-authoritative UI and action gating |
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
| `refresh_strategic_condition` | Recompute morale, pain, blood loss, fear, fatigue, readiness, and check effectiveness |
| `set_character_religion` | Record church conversion or biography renunciation for party Faith relationships |
| `ensure_settlement_activity` | Maintain 3–5 visible quests and 1–2 locally generated recruiting NPC quest parties |
| `start_mission` | Allocate port, record mission |
| **`commit_mission`** | **Apply mission results (XP, items) - idempotent** |
| `cancel_mission` | Cancel active mission |
| `start_quest` / `complete_quest` | Quest management |
| `travel_to_quest` | Advance strategic time and move a party to its off-road quest location |
| `autoresolve_quest` | Run the bounded shared-core melee/ranged simulation, commit final injuries, blood loss, and spent ammunition, retain a seeded summary and expandable combat log, apply Surgery deterioration, and complete or retain the quest according to the outcome |

## adventuresim-tactical-server

The tactical server is a headless Bevy application that:

- Runs as a separate OS process
- Uses Lightyear with WebTransport
- Parses GLB files for spawn markers
- **Maintains ALL tactical state in game memory** (HP, damage, positions, enemies, loot)
- **Commits only the final results** to SpacetimeDB when the mission ends

Strategic incapacitation deliberately excludes tactical imbalance, breath exhaustion, animation state, and knockdown. Only durable inputs and final outcomes cross the boundary: body-part injuries, blood volume, spent strategic ammunition, fatigue accumulated by strategic travel, morale history, encounter results, and diagnostic autoresolve reports. The report records exchanges for explanation and replay; it does not persist live tactical state.

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
spacetime version install 2.6.1
spacetime version use 2.6.1
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

The repository's module and SDK are pinned to SpacetimeDB 2.6.1 and should be
built, published, and used to generate bindings with the matching CLI. This
pre-launch 1.x upgrade deliberately does not support an in-place schema/data
migration: stop the old server, retain an operator backup if wanted, move the
old data directory aside or provision a new empty one, select 2.6.1, and run
`just web-reset`. That explicit startup resets, reseeds, and permanently
discards prior database contents. Once the reset is complete, return to
`just dev` / `just web` and plain `just publish`; ordinary startup and module
updates are non-destructive.

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
