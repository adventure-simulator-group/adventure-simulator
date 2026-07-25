# Architecture MVP - Adventure Simulator

Repository-authored dialogue and quest content share a deployment boundary:
sorted YAML sources are validated and compiled into their crate, then
deserialized once at startup. Services never interpret loose deployment files.
Generated quest authority persists the content digest used to create it.
Canonical truth, hard-zero diagnostics, hidden evidence DCs and factor traces
remain private; catalog authoring does not broaden observer-safe projections.

Persistent local problems and their privacy boundary are specified in
[LOCAL_PROBLEMS.md](LOCAL_PROBLEMS.md). Their strategic authority is stored in
SpacetimeDB; generation and consequence evaluation are pure shared-core logic.
They never persist tactical tick state.

Strategic threat identity and evidence vocabulary live in
`adventuresim-core::bestiary`; see [BESTIARY.md](BESTIARY.md). Stable IDs cross
existing strategic persistence boundaries while display names remain
presentation-only. Tactical servers do not yet consume this identity, and
positions, HP, damage, and enemy instances remain transient tactical state.

Persistent settlement NPC identity, visible profile, and scheduled location presence
belong to the strategic SpacetimeDB layer. Presence is a coarse strategic time/location
fact used for dialogue authorization; it must not contain or mirror tactical positions,
HP, damage, enemies, or tick state. Population-generation explanations are private
diagnostic state and are not subscribed into player-facing web views.
The public presence row contains only settlement, location, schedule, and default-selection
state. Causal bridges and investigative circumstances are private generation facts. The
canonical stable weighted evaluator lives in `adventuresim-core`; production seeding and
tests use the same typed input, hard-zero, bridge-validation, and explanation path.

The strategic dialogue subsystem is documented in [DIALOGUE.md](DIALOGUE.md).
Its compiled catalog and evaluator are shared by the web and SpacetimeDB module;
authoritative sessions are strategic persistence, while free-form chat remains
separate.

Minimal SpacetimeDB implementation for Adventure Simulator.

- **Strategic Layer**: SpacetimeDB for character progression, inventory, parties, missions
- **Tactical Layer**: Bevy/Lightyear game servers for real-time combat (in-memory state only)

Native strategic balance experiments are described in
[`STRATEGIC_SIMULATION.md`](STRATEGIC_SIMULATION.md). They reuse pure
authoritative progression functions from `adventuresim-core` and do not create
production NPC rows or persist tactical state.

## Key Principle

**Tactical gameplay state (HP, damage, positions, enemies, loot drops) lives ONLY in the adventuresim-tactical-server game state.**

Authenticated tactical completion is opaque to strategic objective authority:
the tactical process reports failure or success but never receives or chooses
a case objective, nonlethal result, capture subject, or weight. For successful
case-bound combat, SpacetimeDB deterministically samples a currently valid
result from the mission's private immutable observer-authorized manifest, then
atomically commits the case fact, hostile disposition, capture custody change,
and defeat-only loot. Allied autoresolve uses the same boundary. Random
encounters remain unbound and defeat-only.

## Offline world compilation

The MVP playable area is the exact EPSG:4326 box
`[8.965, 50.877, 11.110, 52.211]`. The shared world-schema constant is the
authority for canonical import, map clipping, terrain lookup, and routing.
Viabundus topology is filtered before environmental enrichment: settlements
and edge endpoints outside the box are not canonical world records. Raster
readers may consume the smallest enclosing whole-degree source cells
`[8, 50, 12, 53]`, but generated manifests and runtime access retain the exact
decimal boundary.

Raw historical and geographic datasets are compiled outside SpacetimeDB by the
native `adventuresim-world-import` crate. Each upstream format has an isolated
source module; the outer builder combines them into a validated canonical
world. `adventuresim-world-schema` contains the dependency-light, versioned
import types shared by the compiler and strategic module. The strategic module
accepts those records through reducers but never parses raw datasets or depends
on native geospatial libraries.

Generated strategic map tiles and terrain-routing packs are a separate data
distribution boundary from the AGPL software. The offline compiler copies the
canonical `MAP_DATA_LICENSE.md` terms beside each bundle, while strategic-web
serves the same notice at `/map/data-license`. Deployments must retain that
notice so source-specific attribution and pass-through conditions travel with
the otherwise optional file-backed artifacts.

Normal development consumes a separately pinned compiled runtime ZIP containing
`world-1544.json`, the AVIF strategic-map package, and the coherent final
terrain-routing package. `just load-world` installs that small immutable bundle
when absent and loads the compiled JSON; raw source initialization and offline
geospatial compilation are release-maintainer workflows, not fresh-checkout
requirements. The runtime archive carries both the strategic-map licence and a
generated notice derived from the compiled world's embedded source manifests.

Every gridded enrichment shares the canonical `SpatialGridSpec` described in
`docs/SPATIAL_GRID.md`. The complete spec and inference-rules version are
serialized in world metadata, so either changing alters the content-addressed
artifact identity. Source coverage extents remain source-manifest concerns.
The grid is compiler metadata, not a SpacetimeDB table shape: no grid columns or
tactical coordinates are persisted.

World schema v21 retains typed canonical
distribution manifests documented in `docs/SOURCE_MANIFESTS.md`. Their
schema/rules/year/grid/source digest is the cache and build boundary and is
retained by the import session alongside the complete artifact ID.

Source modules first parse into importer-only draft types. The outer builder
enriches that draft in dependency order and only then constructs the canonical
world schema. For example, Viabundus supplies settlement identity and road
topology, while GLO-30 supplies the required typed elevation for each draft
settlement. HYDE 3.5 then adds an exhaustive typed land-use profile and constructs
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
to `strategic-web` and never connect to SpacetimeDB directly. The current web
process is explicitly an anonymous, loopback-only, single-user development
surface: its character cookie is a selector, not per-user authentication.
Non-loopback binding requires the clearly named insecure-development opt-in.
`strategic-web` owns a single generated-client WebSocket subscription to the
mutable tables that invalidate strategic UI fragments and fans those database
changes out to selected-character pages as Datastar server-sent events.
Large static world tables, including settlements, routes, aliases, and source
descriptions, stay out of this subscription and are queried on demand.

Strategic terrain is a separate, optional native artifact rather than a
SpacetimeDB grid. The offline compiler preserves the initialized GLO-30 cells
in independently compressed chunks and merges road, water, and forest surface
classes. `strategic-web` streams verified chunk ranges from the pack into a
bounded LRU; it never allocates the continental pack in RAM. Route requests run
deterministic A* on a two-worker blocking pool with a cooperative deadline, an
expanding search corridor, and a bounded normalized-endpoint cache. It submits
the bounded route geometry, terrain spans, package digest, aggregate distance,
and directional travel time to planned travel reducers. Quest journeys persist
separately planned outbound and return legs, and camp redirects replace the
remaining route rather than reusing its old straight-line duration.

Travel and travel-approval reducers accept calls only from the singleton
authenticated strategic gateway identity, and planned routes must match the
package digest that identity registered at startup. Reducers also re-check the
current character, party authority, destination, coordinate/span continuity,
physical distance, and a maximum-speed lower bound before persisting the active
party route. The first authenticated gateway claim is an operational trust
bootstrap: deploy the database privately and start the intended gateway first.
A compromised registered gateway can still forge routes within those semantic
bounds, so its `SPACETIMEDB_TOKEN` is a server credential and must not be shared
with browsers. The tactical server still owns every live
position and terrain interaction; neither raw raster cells nor tactical ticks
are stored in SpacetimeDB.

Browser-facing deployments terminate HTTPS at a reverse proxy and negotiate
HTTP/2 or HTTP/3. Multiplexing prevents the long-lived Datastar SSE stream from
consuming one of the browser's small HTTP/1.1 per-origin connection pool while
navigation and component refresh requests are pending. The Rust web process
remains an internal HTTP service; local development uses `Caddyfile.dev` to
expose it at `https://localhost:8443`.

The SSE stream patches a stable, server-rendered revision marker. Strategic UI
components subscribe to that marker and refresh only their relevant state. This
drives canonical-location navigation (including en-route camps), party portraits, party requests and
notifications, recruitment roles and applicants, inventories and loot, map and
quest rails, selected-character details, map quest markers, service
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

Finalized loot is strategic state. The tactical server derives drops from the temporary enemies' equipped inventory and records only the resulting item identifiers and quantities. The strategic layer owns the post-battle result, shared party inventory, and per-character value stakes; no enemy, damage, position, or other tactical tick state is persisted. Mission, battle, hostile-group, and outcome-source identity are explicitly separate from contract/quest identity; see [MISSION_AUTHORITY.md](MISSION_AUTHORITY.md).

NPC recruitment and strategic interruptions likewise have dedicated strategic
authority. Recruitment offers own company discoverability, expiry, and
eligibility without accepting a quest. Incidents own typed source, site,
hostile-group, and lifecycle identity without creating a quest or replacing a
party's active contract. See
[RECRUITMENT_AND_INCIDENT_AUTHORITY.md](RECRUITMENT_AND_INCIDENT_AUTHORITY.md).

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

### Disease facts and derived physiology

Disease belongs to the strategic layer and is evaluated against each patient's
exact personal character minute. An infection episode persists only its stable
identity, character and disease associations, `contracted_at`, and optional
`treated_at`. Phase, severity, symptoms, diagnosis, impairment, and recovery are
pure deterministic derivations in `adventuresim-core`; resolved and repeated
episodes remain as history. Definitions use a numbered ruleset.

Alchemy recipes are static ruleset data. Their ingredients remain ordinary
personal or shared inventory rows. Crafting produces a transferable medication
item; equipping consumes that item into an `equipped_medication` row and sets the
matching episode's `treated_at` timestamp. This public row exposes only the
course being taken, not undiagnosed infection history, and is removed on
unequip or deterministic recovery cleanup.

Every settlement persists a deterministic `settlement_herbalist` service row
whose Medicine rank is always 2–4. NPC examinations are settlement-bound,
charge personal gold, and advance only the patient's clock by 15 minutes. The
medical-only time advance performs disease clipping and capability refresh but
does not apply travel hunger, thirst, fatigue, or religious-observance effects.
The private one-shot result contains only parallel canonical disease and
medication names; it never reuses the richer player-doctor examination model.
The trusted web route reads that narrow result through
`backend_herbalist_examinations`, returns only name pairs to the authenticated
patient's dialogue, and dismisses it. The patient's request and every result
line render transiently in that browser and never enter durable party-scoped
local chat. Prepared medication purchases use a separate herbalist reducer,
remain forbidden to generic merchant trade, and create quantity-one personal
inventory rows.

Every time advance inspects its whole interval. A terminal respiratory,
circulatory, homeostatic, or neurologic failure clips travel, rest, training,
and lazy catch-up at the exact boundary before using the shared idempotent death
transition. Zero Endurance, Gut, Intelligence, or Instinct is impairment, not a
direct death rule.

Outbreaks are public world facts. Infection episodes and committed-cut
provenance are private strategic facts. Tactical code may commit final cut
damage but never tactical positions, HP, enemies, or tick state. Strategic-web
is the medical presentation boundary and gives templates only a viewer-sanitized
model; raw medical rows are never browser payloads.

  The `backend_infection_episodes`, `backend_committed_cuts`,
`backend_medical_examinations`, and `backend_herbalist_examinations` views are narrow SSR integration surfaces, not
browser APIs or subscriptions. Deployments keep
the strategic SpacetimeDB endpoint on the server network; strategic-web applies
the active character's Medicine gate before emitting HTML.

The strategic web process is the trusted presentation and action boundary.
Medical queries fail closed and visibly; medical action routes derive the acting
character from the web session before invoking reducers. Public browser
subscriptions never contain infection episodes, pending examination results, or
disease notices.
Public subscriptions never contain infection episodes or disease notices.

Tactical requests bind the requesting leader and authoritative party to a
one-use request row. Dispatcher-created servers inherit that party binding, and
ordinary characters may enroll only when their current party matches it.
Persistent quest XP is derived by the strategic quest completion path; tactical
servers cannot supply an arbitrary XP award.

### Observer-specific investigations

Procedural case construction is specified in
[QUEST_GENERATION.md](QUEST_GENERATION.md). A bounded shared-core weighted
constraint solver produces one private typed manifest; the strategic reducer
atomically materializes its symptom-only local problem, witnesses, evidence,
actions, DNF objectives, custody, hostiles, finales, and replay trace.
Canonical case identity remains separate from public observer knowledge.

Canonical case events and every testimony stage are private SpacetimeDB state;
per-character beliefs, revisions, and leads are private too. The registered
strategic SSR gateway receives only sanitized `backend_investigation_*` views
and queries them for the active session character. Browser live subscriptions
never include raw investigation tables. See
[INVESTIGATIONS.md](INVESTIGATIONS.md).

Compiled dialogue may contain typed runtime slots, but the strategic reducer
resolves them from persistent NPC, presence, and observer-safe investigation
authority before a transcript event is stored. Dialogue-driven objective
producers are private owning-subsystem calls with live-recipient and knowledge
checks; they do not create a generic browser fact-ingestion path.

Strategic investigation actions follow the same boundary. Private capability,
area, attempt, and outcome tables own targets, geometry, deterministic inputs,
and success state. The registered SSR gateway exposes only observer-safe action
and outcome projections and accepts an opaque action ID, method, and expected
version. Party members route the same request through party consent. The
authority revalidates ownership, prerequisites, party state, and location
before consuming strenuous strategic time. Generated public IDs use
domain-separated SHA-256 tokens derived from separately sampled private
observer entropy; resolution seeds are independently sampled and refreshed for
new capability versions. Surveillance and ambush preparation
remain strategic and never persist tactical positions, damage, HP, enemies, or
fabricated hostile dispositions. Drive-off and capture require the
authoritative tactical producer tracked in #207.

Physical-evidence inspection follows the same observer boundary. The immutable
generated evidence authority owns the inspection topics, checked attribute,
and creation-time fixed difficulty. Gateway views expose an object only while
the observer has exact knowledge of and occupies its case site, and serialize
only its safe presentation, topic IDs and labels, and completed narration.
The reducer revalidates location and compares the authoritative character
attribute directly with the hidden threshold; there is no inspection-time
randomness and no tactical state.

Action graphs retain private predecessor and alternate-route edges. The
authority validates those edges within one observer and case, synchronizes the
co-located party's strategic clocks, then uses the leader/party minute for
light, evidence age, and recorded outcomes. Custody and hostile-resolution
effects are committed only by their owning strategic adapters.

### Strategic tables

| Table | Description |
|-------|-------------|
| `character` | Character progression, location, and life state; no tactical tick state |
| `character_limbs` | Health projection derived from durable per-limb strategic injuries |
| `limb_injury` | Per-character, per-limb cut, bruise, fracture, bandage, stitch, and applied-splint state |
| `retained_projectile` | Durable retained arrowhead/ball records with extraction difficulty; no tactical anatomy or tick state |
| `character_condition` | Durable strategic blood volume, body weight, and religion selection |
| `character_filth` / private filth provenance and disease snapshots | Public bounded dirt/blood deposits expose only Own/Foreign/Unknown; exact source IDs and blood-compatible source episodes remain private |
| private travel/blood checkpoints | Fractional travel-dirt progress and committed blood-exposure evaluation prefixes used to make chunked time advance deterministic |
| `settlement` | Strategic settlement data, including its legal religious status and current church |
| `morale_event` | Time-stamped strategic successes and setbacks with seven-day decay |
| `character_morale_source` | Refreshable named, signed contributions used by the morale meter breakdown |
| `character_strategic_condition` | Refreshable derived morale, ally-restoration percentage, and incapacitation projection for server-authoritative UI and action gating |
| `character_personality` | Nine immutable strategic personality axes, including Temperance; one typed row per newly created character |
| `alcohol_consumption` | Durable per-character/per-evening fixed-point ethanol history and idempotent morale-evaluation marker |
| `inventory_item` / `item` | Persistent concrete stacks and explicit definitions; alcohol serving volume, ABV, net hydration, medical protection, and disinfectant effectiveness are definition data rather than inferred IDs |
| `character_affinity` | Directional subject-to-actor signed anchor, lazily decayed on the subject's personal strategic clock |
| `character_familiarity` | Canonical symmetric pair and shared-party strategic minutes |
| `social_belief` | Private observer-specific perceived personality axis, confidence, and observation minute |
| `social_interaction` | One durable record per authoritative social attempt and its realized morale outcome |
| `party` | Party groups, active quest, and aggregate skill-check targets; every character belongs to at least a solo party |
| `party_member` | Party membership, including the recruitment role that filled a slot |
| `party_recruitment_role` | Named party-independent role requirements and slot quantities |
| `saved_recruitment_role` | Reusable named role requirement presets owned by a character |
| `party_join_request` | Pending applications to a recruitment role |
| `party_action_request` / `party_leader_vote` | Persistent member suggestions and one standing leadership vote per living member |
| `character_death` | Immutable first death cause/source and personal strategic minute, keyed by character |
| `local_chat_message` | Party-scoped NPC and face-to-face player conversation history |
| `character_capability` | Cached automatic equipment, attribute, skill, and mobility tags |
| private `mission_authority` | Party, scene, optional case-site, and optional hostile-group binding for one combat opportunity |
| private `hostile_group_authority` | Persistent case-site occupant and its defeated state |
| private `outcome_source_authority` | Authenticated, idempotent source receipt for one victorious strategic outcome |
| private `tactical_server_claim` | One-use SHA-256 dispatcher claim bound to one mission request |
| `battle_result` | Battle/source attribution, never keyed by quest |
| `battle_loot_item` / `battle_participant` | Battle-keyed loot and eligible participants |
| private `case_authority` / `case_outcome_fact` / `case_outcome` | World resolution authority, typed source-idempotent facts, and one final outcome; separate from investigation truth and tactical ticks |
| private `quest_generation_authority` | Catalog revision, deterministic context, canonical manifest, and factor/backtrack trace for private replay |
| private `contract_authority` | Offered, accepted, reported, and paid agreements concerning a case; acceptance never creates or deletes the case |
| private `case_custody` | One versioned current holder per case asset or subject |
| `backend_contracts` view | Trusted-gateway contract projection; browsers receive only server-selected observer-safe presentation |
| private `case_site_authority` | Stable case-site identity, origin, scene, distance, and physical coordinates |
| private `party_case_site_tracking` | Per-party presentation/navigation selection; it grants no knowledge, acceptance, objective progress, or reward |
| gateway-only `backend_case_site_pins` | Observer-owned exact site projection, emitted only from unrevised exact/visited investigation knowledge |
| public `character` | Public identity and current settlement only; case-site occupancy is never globally subscribable |
| private `character_case_site_occupancy` | Typed per-character `CaseSiteId` occupancy authority |
| private `party` / `party_journey` / `party_journey_route` authority | Typed party occupancy, endpoints, camp destination, and exact route geometry |
| gateway-only party/journey/location views | Owner-facing movement presentation for strategic-web and the simulator |
| `port_allocation` | Tactical server port allocation (singleton) |

`character_personality`, `character_affinity`, `character_familiarity`,
`social_belief`, and `social_interaction` are private module tables. Gateway-
guarded views expose relationship state and observer beliefs only to the trusted
SSR identity and return no rows to other callers; browsers do not subscribe to
true personality or relationship history. Social reducers require that same
gateway identity, derive a closed topic from a current negative morale source,
and revalidate living state, party membership, co-location, source ownership,
and a topic/action cooldown. External attempts write one interaction and one
morale event atomically; self-only Reflection updates a self-belief without
changing Affinity or Familiarity. Strategic relationship state
never contains tactical positions, HP, damage ticks, or enemies.

This is a pre-launch clean schema change. Development databases must be
recreated/reseeded; there is intentionally no Charisma compatibility field,
migration, dual-read path, or preservation of disposable characters.

### Key Reducers

| Reducer | Description |
|---------|-------------|
| `upsert_character` | Create/update character (gives starter items) |
| `add_item_to_inventory` | Add items |
| `backfill_solo_parties` / `leave_party` / `disband_party` | Maintain the invariant that every character has a party |
| `backfill_character_deaths_and_leadership` | Non-destructively supply legacy death records and normalize stale/missing standing votes |
| internal `transition_character_to_dead` | Idempotently commit a durable death outcome and trigger leadership reevaluation |
| `create_recruitment_role` / `update_recruitment_role` / `delete_recruitment_role` | Create, resize, edit, and remove grouped party recruitment slots |
| `save_recruitment_role` / `delete_saved_recruitment_role` | Manage reusable role presets |
| `update_party_check_targets` | Configure non-filtering Medicine, Command, and Religion aggregate goals; surgical capability is an individual Anatomy/Knife/Tailoring composite |
| `upgrade_manual_surgery` | Idempotently adopt legacy limb deficits into injury rows and upsert surgery item definitions |
| `treat_limb` | Perform one individual bandage, stitch, splint, splint removal, or projectile extraction with participant-local time |
| `request_to_join_party` / `accept_party_join_request` / `reject_party_join_request` | Role recruitment and atomic party merging; destination leadership remains intact while source members, pooled assets, and stakes transfer |
| `request_general_party_join` | Submit a retained application through a shared zero-capacity Unassigned role |
| `send_local_chat_message` / `record_local_npc_message` | Persist location-gated, party-owned Local conversations |
| `refresh_capabilities` | Recompute automatic character tags through the shared core evaluator |
| `refresh_strategic_condition` | Recompute morale, pain, blood loss, fear, fatigue, readiness, and check effectiveness |
| explicit rest reducers | Require the registered strategic gateway (or the owner of the target disposable simulation character), validate the physical rest location and one-year work bound, atomically plan washing, then advance rest and nightly alcohol chronology |
| `set_character_religion` | Record church conversion or biography renunciation for religious relationships |
| `ensure_settlement_activity` | Maintain 3–5 visible quests and 1–2 locally generated recruiting NPC quest parties |
| `start_mission` | Allocate port, record mission |
| **`commit_mission`** | **Apply mission results (XP, items) - idempotent** |
| `cancel_mission` | Cancel active mission |
| `accept_contract` / `abandon_contract` / `report_contract` | Separate contract lifecycle; direct bounties disclose their seeded case site on acceptance and pay exactly once after case resolution |
| `track_case_site` | Select an already-known exact site for party navigation without accepting a contract, moving, progressing an objective, or paying a reward |
| `travel_to_case_site` | Authorize through observer-safe exact knowledge, advance strategic time, and move a party to the typed off-road case site |
| `autoresolve_quest` | Run the bounded shared-core melee/ranged simulation, commit per-hit cut/blunt/projectile facts into manual limb injuries, blood loss, and spent ammunition, retain a seeded summary and expandable combat log, and complete or retain the quest according to the outcome |
| `treat_limb` | Align one treating character and patient on their personal clocks, advance only those participants, and perform one validated Anatomy-based projectile-removal, bandage, stitch, or splint procedure |

The current strategic module does not yet persist a player-identity-to-character ownership mapping.
Most strategic reducers therefore rely on the authenticated strategic gateway and simulator's
database connection as a system-wide trust boundary; character IDs alone are not authorization.
The public rest and surgery reducers enforce that boundary directly: only the registered gateway
may mutate a normal character, while a simulation-run owner may act only through a character
registered to that disposable run. Settlement rest additionally derives tavern eligibility from
the character's persisted settlement presence rather than trusting a caller flag.
Reducers that already have a concrete identity relationship (world imports, simulation runs,
tactical servers, and religious-demand ownership) validate `ctx.sender()` directly. Equipment
repair follows the existing strategic boundary until ownership is introduced consistently for all
player-facing strategic reducers.

Tactical registration is request-backed and bound to the authoritative party
and quest. The registering identity becomes the sole authority for enrollment,
temporary characters, departure, and completion; direct registration and
replacement are not exposed. The local dispatcher-to-child launch currently
retains one explicit internal-network assumption: the SDK launcher does not
pre-provision an expected child identity or per-request bearer token, so an
identity able to read a pending request could race the child for the first
claim. Request deletion and identity checks prevent reuse and cross-server
calls after that claim. Keep the database endpoint and request stream off
untrusted networks until expected-identity provisioning is implemented.
Client join messages currently name an existing character because the netcode
does not yet carry a player-to-character credential. The tactical server never
creates a character from that value, and the module admits only a living member
of the mission's authoritative party. Until character ownership is added, the
client connection remains an explicitly trusted mission-local boundary.

Queued party actions deliberately have parallel Rust DTOs in strategic-web and
the SpacetimeDB module: sharing the module type would couple the web process to
the server-only SpacetimeDB macro/runtime surface. A cross-boundary contract test
locks the complete variant-to-kind mapping so either side cannot drift silently.

## adventuresim-tactical-server

The tactical server is a headless Bevy application that:

- Runs as a separate OS process
- Uses Lightyear with WebTransport
- Parses GLB files for spawn markers
- **Maintains ALL tactical state in game memory** (HP, damage, positions, enemies, loot)
- **Commits only the final results** to SpacetimeDB when the mission ends

Strategic incapacitation deliberately excludes tactical imbalance, breath exhaustion, animation state, and knockdown. Only durable inputs and final outcomes cross the boundary: body-part injuries, blood volume, spent strategic ammunition, equipment condition, fatigue accumulated by strategic travel, morale history, encounter results, and diagnostic autoresolve reports. Tactical sessions receive condition-adjusted equipment snapshots, but tactical tick impacts remain transient until the tactical result handoff grows an explicit equipment-wear summary. Autoresolve commits its final equipment wear directly. The report records exchanges for explanation and replay; it does not persist live tactical state.

### Command-Line Arguments

```bash
adventuresim-tactical-server \
  --addr 127.0.0.1:6000 \
  --mission-id "mission-123" \
  --scene-key "town_a" \
  --required-enemy-kills 4 \
  --spacetimedb-url "http://localhost:3000" \
  --spacetimedb-module "adventuresim-stdb-module"
```

### Mission End Flow

When the tactical mission ends (timeout, victory, or defeat):

1. The dispatcher snapshots the quest's authoritative enemy count into the
   request and launches the server with that required objective.
2. The tactical server counts each mission enemy only from its internal,
   authoritative death event and succeeds only after the entire objective.
   The current combat prototype does not yet apply damage or emit that death
   event, so timeouts fail closed rather than granting a false victory.
3. Tactical server calls SpacetimeDB `end_tactical_server` reducer:
   ```rust
   end_tactical_server(success, reported_xp)
   ```
4. SpacetimeDB derives persistent XP from the quest, records final loot, and
   applies rewards only to living party members.
5. Tactical server terminates.

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
`just dev` / `just web`, which reset local data only for breaking schema
changes, and plain `just publish`, which remains non-destructive.

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

## Strategic random encounters

Random encounters are canonical journey events, not tactical state. A private
authority row persists one journey entropy seed and its next three-hour roll
cursor; neither value is exposed through subscriptions. Terrain, day/night,
and distance plus matching enemy archetype for that party's accepted active
quest affect bounded selection rolls. An interruption row persists the exact route position,
movement/elapsed/absolute minute, awareness result, available typed choices,
and surrender preview. It never persists enemy HP, positions, or combat ticks.

Unresolved encounters guard travel, rest, party membership, quest abandonment,
equipment, and inventory mutations that could bypass or invalidate them. Combat
uses the shared final autoresolve commit path for wounds, blood, ammunition,
equipment contact wear, filth, morale, loot, and diagnostics. Random victories
remain separate from `BattleResult` and quest completion.

## Language persistence

Language is strategic state. Compiled settlements persist a versioned, deterministic vernacular profile inferred inside the exact playable bounds; the three German shares total exactly 10,000 basis points. Characters persist direct Oral and Written hours. Effective proficiency is derived once from symmetric correlation matrices and is never recursively stored. The importer CLI can inspect a coordinate with `--infer-languages LONGITUDE LATITUDE`.

Rules-v9 adds two immutable gameplay projections. A bounded settlement economy
profile combines population, route access, documented town status, and the
canonical industry profile into prosperity, service availability,
specializations, and relative stock categories. Every gap-fill stock fact is
typed as deterministic fabrication rather than attributed to an upstream
dataset. Authoritative reducers consult the profile; it is not a UI-only hint.

Road inference uses a two-stage artifact contract. The documented-base terrain
pack contains only Viabundus roads plus source-mapped water, forest, elevation,
and Jung wetlands. World compilation runs bounded A* against that immutable
digest, so a proposed road cannot lower its own cost. Accepted polylines are
stored in schema 25 with explicit inferred provenance. Final map generation
requires the same base digest and feeds those exact polylines to both the visible
quiet road layer and the final routing road mask; both identities are recorded.
Jung v1.1 wetland posterior/categorical pixels are bounded to playable coverage;
water remains impassable, roads take precedence, and other wetland cells use the
distinct slow terrain surface.
