# Architecture

Ingredient preparation and medicinal transformation are strategic reducers.
They persist measured substance lots, exclusive preparation state, private
versioned medicinal components, and stable-container passive processes.
Tactical positions, damage, HP, enemies, and other tactical tick state remain
transient.

Static item definitions cross the strategic boundary as a build-time embedded
catalog. YAML is authoring input only; the flattened SpacetimeDB `Item` table
is its deterministic persistence/client projection. Inventory ownership,
custody, amount, and condition remain strategic state and are not definition
content. See [Item definition authoring](../contributing/item-authoring.md).

Fabelgeist separates persistent strategic play from transient
real-time tactical play. The boundary is architectural, not merely a difference
between screens:

- **Strategic state** lives in SpacetimeDB and advances through discrete,
  authoritative actions.
- **Strategic presentation** is server-rendered by `strategic-web` with
  Axum, Maud, and Datastar.
- **Tactical state** lives only in a short-lived headless Bevy server and is
  replicated to the Bevy client over server-authoritative WebSockets.
- **Shared rules** live in dependency-light Rust crates so strategic
  autoresolve, tactical play, validation, and tools can use the same
  calculations without sharing persistence.

## Persistence boundary

SpacetimeDB stores durable world and character facts such as:

- characters, parties, progression, schedules, injuries, and needs;
- separate character wetness and signed thermal strain, with sustained
  frostbite committed through the durable limb-injury authority;
- inventory, equipment condition, currency, and custody;
- settlements, organizations, journeys, cases, contracts, investigations, and
  private puzzle authority with observer-safe challenge projections;
- mission requests, authenticated outcome receipts, final battle results, and
  finalized loot;
- compiled world identity, source manifests, and strategic route facts.

Equipped inventory is persisted as a normalized acyclic graph. Root nodes
occupy ordered body anchor/channel cells; child nodes occupy capacity cells on
item-authored attachment points. The equipped header records its stable
placement ID, while occupancy rows record every selected body anchor and every
parent edge. One placement may therefore span several body locations, require
several parent attachment points, or combine both. These normalized rows are
also the sole authority for held items; there is no separate left/right-hand
equipment record. Mutations preflight every conflict, compatibility, capacity,
and cycle check before changing rows, and never leave orphaned children.
Combat protection is a separate explicit placement-to-body projection rather
than an inference from physical anchors.

Tactical servers keep positions, movement, enemies, attacks, per-tick damage,
temporary health, physics state, and scene entities in memory. Those values are
not mirrored into SpacetimeDB. A tactical session may report an authenticated
terminal result, but strategic authority decides which durable consequences
that result is allowed to create.

Random encounters are durable strategic journey events only at their
boundaries: timing, route position, available choices, and final outcome may be
persistent, while the combat simulation remains transient.

Strategic weather exposure is evaluated from absolute character time and
authoritative location after each actually elapsed interval is clipped and
committed. Its pure minute reducer is partition invariant. Settlement
coordinates use imported elevation; route and private case-site coordinates
use zero elevation until those authorities carry elevation. Field shelter is a
typed action input: a bivouac provides no protection, while a party-owned tent
blocks rain and reduces wind without generating heat. Route terrain currently
has no ford locations, so water immersion is an explicit reusable impulse and
is never guessed from generic wetlands.

## Strategic authority

The SpacetimeDB module owns authoritative mutations. Reducers validate the
current party, character, location, time, custody, and source identity before
changing state. Shared-core functions perform deterministic calculations, but
do not grant authority by themselves.

Contextual Character interactions share only a pure typed decision—allowed
with a narrow reason, refused, or unavailable. Domain reducers still own exact
presence, privacy, procedure, retry, and commit validation. This is not a
universal permission table: medical emergency doctrine remains in Surgery and
contact consequences remain in social/context authority.

Hostile-group terminal outcomes share a battle-independent exact commit seam.
Tactical victory may surround that seam with battle results, participant
morale, corpses, and loot; accepted pre-combat withdrawal calls only the seam
and emits the existing `HostilesDrivenOff` case fact. This keeps durable case
resolution strategic without inventing tactical state for a conversation.

Strategic systems share a closed, versioned vocabulary for place and fixture
identity. A coarse settlement identity is not an exact venue, and constructing
an inn, chapter, residence, case-site, camp, source, or fireplace identity does
not establish existence, presence, visibility, knowledge, ownership, or
permission. See
[Strategic place and fixture identities](world-data/strategic-places.md).

Consequences shared by multiple strategic domains may pass through a closed,
versioned world-event envelope. The envelope records the directed actor and
subjects, canonical settlement jurisdiction, occurrence minute, exact source
identity, and a typed domain payload reference. It is private canonical
authority: it is not an observation, knowledge grant, permission, public log,
subscription, or asynchronous bus. Rights and knowledge decisions still occur
in their owning reducers before an envelope is committed.

The private event receipt first exact-matches a digest of stable reducer inputs,
before reading mutable participant or consequence state. An exact retry is a
successful no-op even if party membership later changes; reuse of an event ID
with different fame, infamy, source, place, time, or other request provenance
fails. On first application, the receipt also binds the complete envelope,
affected-character snapshot, and closed ordered consequence plan. Every
pre-existing subordinate row must exactly match its immutable authored fields
before any consequence runs, which permits safe adoption of matching legacy
rows but rejects collisions. New consequences run in their established order
inside the originating SpacetimeDB transaction, and the receipt is inserted
last so a failed consequence leaves neither partial state nor replay authority.
Reputation continues to use its existing immutable-event spillover formula and
caps; private offenses and local-problem outcomes retain their existing domain
tables and visibility.

Errantry challenges follow this boundary. The private seed and canonical
assignment remain in durable strategic authority; the gateway receives only
formal clues, allowed interactions, presentation, revision, and safe feedback.
Attempts are revisioned durable receipts. Solving emits a typed strategic case
fact and never persists tactical positions, damage, HP, or enemies.

Multi-character strategic time actions snapshot disease exposure before the
first participant clock mutates. The reducer-local plan projects only the
explicit co-advancing set, prefetches bounded pair-presence coverage, and gives
preview and commit the same deterministic acquisition proposals. Solo
catch-up reads indexed recorded history and caps open-span overlap at the
peer's already-elapsed clock. Route candidates are resolved simultaneously
within each absolute minute and become contact sources on the following
minute.

The largest strategic implementations are organized behind their established
Rust module paths. `strategic` and `investigation` retain ordered same-scope
source fragments because SpacetimeDB table, reducer, view, and generated
accessor discovery is module-scope-sensitive; those fragments are not ordinary
Rust child modules. Quest generation and the live strategic simulator use
ordinary child modules where their dependency boundaries permit it. The web
settlement router keeps route registration in a small facade and uses ordinary
domain modules for handlers, forms, rendering, rest-preview policy, and
behavior-local tests.

The current deployment boundary is intentionally narrow:

- `strategic-web` holds the registered strategic-gateway identity;
- browsers submit ordinary HTTP actions to `strategic-web` and never receive a
  SpacetimeDB credential;
- one signed, opaque `adventuresim_session` bearer cookie identifies a
  pseudonymous browser owner; character IDs and rosters never enter cookies;
- private grants bind each generated character to exactly one browser owner,
  and a private selection row chooses presentation context;
- non-loopback use therefore requires the explicit insecure-development
  opt-in until account ownership is implemented;
- tactical dispatch claims are one-use, digest-bound capabilities rather than
  copies of the gateway token.

Observer-specific truth remains private. Browsers receive only gateway-filtered
views of contracts, dialogue, investigations, evidence, physiology, and
case-site knowledge. A public or subscribed row is not automatically safe to
use as an authorization decision.

Every persistent settlement resident is an ordinary full `Character` with one
authoritative `u64` character ID, the usual component rows, one
`CharacterTime`, and an `NpcPolicy`. There is no parallel NPC person record or
string identity. `SettlementResidentProfile`, keyed by that character ID,
contains only settlement-specific appearance, vocation, service, and
conversation metadata; presence is keyed by the same ID. Recruitment adds that
same Character to a party rather than materializing a replacement.

The canonical `Character` table and its identity-bearing durable component
tables are private. Only the registered strategic gateway may subscribe
broadly through `BackendCharacters` and the corresponding backend component
views; tactical servers continue to receive only their `ConnectedPlayer`
view. This prevents a direct client from enumerating a globally exclusive NPC,
or detecting a future birth through an otherwise ordinary time, physiology,
skill, or needs row, while that lifecycle fact is still beyond the selected
character's personal date. Death receipts and derived morale-source labels are
private for the same reason: only gateway projections may read them broadly,
and player-facing death presentation must apply the selected observer's
personal-date boundary. Strategic web therefore reconstructs `Character.alive`
for party, remembered-character, and settlement-resident presentation from the
private death receipt and the selected observer's `CharacterTime`; a missing
observer frontier is treated as insufficient authority to reveal the broad
current death state.

The gateway resident roster is an explicit player-visible join over Character,
private personality, resident profile, and presence authority. It includes the
decimal character ID, home settlement, visible description, occupation,
household, local role, service, and conversation routing. It omits private
demographic sex and the internal projection traversal key. A missing Character
or private personality component fails closed rather than falling back to
duplicated profile data. Browser quest discovery builds its candidate and
commitment from visible age, presentation, profession, role, and presence.
Presentation is committed as seen but is not interpreted as private sex; the
public developer flow leaves that selector empty.

## Strategic web

`strategic-web` renders complete HTML documents for direct requests and
no-JavaScript clients. In an enhanced session:

1. one document-scoped Datastar SSE connection carries invalidation revisions;
2. same-origin strategic navigation requests a server-rendered replacement for
   the stable `#strategic-page` root;
3. enhanced forms execute the same reducer as ordinary forms and receive the
   redirected strategic root in the original response;
4. native links and `303` redirects remain the fallback.

The web process owns one generated SpacetimeDB SDK WebSocket connection. Its
explicit subscription invalidates live regions and supplies a small,
typed public read cache. Private and owner-scoped page data remains
authenticated on-demand SQL until a page has a dedicated authorization-scoped
read model. See [Strategic read cache](strategic-read-cache.md).

First-character onboarding is a separate entry surface. The browser tab holds
a private seed for deterministic candidate previews keyed by life stage.
Confirmation sends only the generator version, seed, selected age tier, and
slot; strategic authority regenerates and persists that exact candidate.
Confirmation atomically grants the generated character to the pseudonymous
browser owner. The strategic header resolves its selected character and roster
only through the gateway-only `BackendBrowserCharacterAccess` projection.
Replaying the same deterministic candidate coordinates is idempotent for that
owner and rejected for every other owner. Switching clears only the server-side
selection; it does not discard grants or expose IDs in the cookie.

The cookie is
`v2.<random 32-byte base64url ID>.<issued Unix seconds>.<HMAC-SHA256>` and is
`HttpOnly`, `SameSite=Lax`, and optionally `Secure`. The signature covers both
the ID and issue time; the server rejects sessions older than 30 days and issue
times more than five minutes in the future. The web server derives a
domain-separated SHA-256 owner key from the verified random ID and never stores
the raw token in SpacetimeDB. Invalid signatures and expired sessions are
anonymous; backend resolution failures fail closed.

All browser POST, PUT, PATCH, and DELETE routes in both onboarding and the
selected-character application require an exact same-origin `Origin` matching
the request Host and effective HTTP scheme. Missing, opaque (`null`),
cross-port, cross-scheme, and otherwise foreign origins fail closed. Internal
strategic navigation is read-only and does not bypass this mutation boundary.

### First-character authority

First-character generation is a versioned strategic authority boundary. The
age tier (`young`, `adult`, or `old`) is part of the GET, POST, reducer claim,
and generated-ID coordinates alongside the private session seed and slot.
Professional previews are regenerated from the organization catalog and the
same package is persisted on confirmation; the browser never submits trusted
attributes, skills, inventory, religion, or membership rows. Creation inserts
the character, skill and equipment package, current organization membership,
presentation, dues, and required professed religion in one reducer
transaction. Tactical state is not involved.

The public confirmation reducer is restricted to the registered strategic
gateway. Membership timestamps are anchored to the character's initialized
strategic minute, including the first paid dues interval, rather than assuming
that a new character always starts at minute zero.

## Tactical lifecycle

The current tactical stack uses Bevy Replicon over Aeronet WebSockets:

1. strategic authority creates a pending tactical request bound to a party,
   scene, mission authority, and required objective;
2. the dispatcher receives the request through its SpacetimeDB subscription,
   provisions a one-use claim, and starts a headless tactical server;
3. the child consumes that claim, registers its identity, and opens the
   WebSocket server;
4. party members connect through the Bevy client and send input;
5. the tactical server keeps all live simulation state in memory;
6. the child calls `end_tactical_server` with its terminal resolution;
7. SpacetimeDB validates the registered server and mission, selects any
   compatible private strategic outcome, and commits durable consequences
   idempotently.

Tactical terrain authority enters the short-lived server through the bounded,
versioned `TacticalSceneInput` document. It contains a deterministic seed,
template key, imported-package or named-fixture provenance, geographic/time
origin, complete strategic weather snapshot, dense playable samples, and
progressively coarser presentation-only vista samples. The server validates the
document before opening any listener, deterministically upsamples coarse source
grids to at most two-metre spacing, adds bounded seeded microrelief, builds its
collider from those row-major heights, replicates the heightfield data, and logs
the SHA-256 scene digest and generation version. Each client builds the playable
render mesh from that replicated heightfield; the server never extracts or
transmits render geometry. Gameplay-relevant generated trees and rocks are
static server colliders with compact replicated recipes and transforms. A rock
recipe contains a deterministic seed, broad archetype, lithology, dimensions,
and conservative collision radius. Clients run bounded uniform-grid Surface
Nets to derive each rock mesh, while the server creates only the recipe and
sphere proxy. The same client-only mesher supplies shared non-colliding
loose-stone variants. Trees likewise derive a seeded four-order skeleton on the
client without receiving or simulating the collider. The immutable world-data canopy coverage continuously controls tree
architecture without inspecting neighboring entities: low coverage produces a
short clear bole and broad open-grown crown, while dense canopy produces a
taller clear bole and narrower competitive crown, so tree generation remains
deterministic and parallel. Tree presentation cross-fades from branch tubes and individual leaf
cards through twig, small-branch, and crown-branch impostors to a single
camera-facing whole-tree billboard. Impostor bounds come from the actual
descendant branch groups rather than an unrelated generic crown, preserving
the generated tree's silhouette across levels. Renderable trunk geometry is a
sibling of the aggregate levels beneath a non-rendering obstacle root, so
disabling the trunk cannot hide its billboard through inherited visibility.
The camera-facing far card bypasses only CPU frustum culling because its final
orientation is produced in the vertex shader from stale source-card bounds;
the normal projected-size visibility range still culls it by distance. Those visual levels do not
change the server-owned trunk collider. The near-tree skeleton also grows a
bounded five-to-ten-root visual flare from the same tree seed. Unequal angular
gaps, reach, radius, and burial break up radial repetition; two or three broader
buttresses follow the main scaffold-load azimuths, and at most two roots add a
short lateral fork. This presentation detail remains capped at 22 segments and
does not expand the server's cylindrical trunk collider. Oak and hazel branch
recipes also cap each child base at 80% of its local parent-axis radius. The
existing bounded basal ring therefore cannot become wider than its trunk or
parent limb. Near woody meshes share one scanned oak-bark albedo, OpenGL
normal, roughness, and ambient-occlusion material. Their cylindrical UVs use
the scan's documented one-by-two-metre scale, duplicate the wrap seam, and
parallel-transport the tangent frame along each axis; physical feature scale
therefore stays consistent across trunks, roots, and branches without a
tree-specific lighting adjustment.
Non-colliding grass and understory use automatically instanced shared meshes,
layered shader wind, and root-to-tip shading. Grass cross-fades from a
2,916-blade, fifteen-vertex near-field macro patch to a stable 144-blade,
seven-vertex subset at distance; rejected blades are absent from the far mesh
rather than collapsed after vertex shading. The 3.2-metre patch spacing cuts
grass render entities by roughly an order of magnitude while retaining the
original macro-patch footprint at four times the authored blade density near
the player and camera. The distant mesh retains the earlier density because
individual blades are subpixel there. The 4x near mesh cross-fades to the
original far topology over 18--26 metres rather than paying four times the
vertex cost throughout the former 34--44 metre high-detail radius. A
deterministic scalar mask derived from
the same authoritative ground-cover contour as the terrain rejects blades on
dirt and leaf litter and progressively thins the grass-side boundary. Boundary
macro patches remain present, while patches fully inside non-grass cover are
omitted. Near and far LODs use the same stable per-blade thresholds, so the
organic edge does not change at the cross-fade. Macro patches
remain unit-scale and nearly gridded, with boundary blade rows constrained to
wander outward to mitigate square seams on near-flat and ordinary sloped
terrain. This is a continuity mitigation rather than a guarantee across sharp
terrain-normal discontinuities. Within the unchanged topology,
deterministic mixed-age height, independent width, clumping, pigment, lean, and
curvature variation avoids a repeated vertical-curtain silhouette. A shared
world-space meadow field keeps the full authored density within seven metres,
then introduces short juvenile pockets and irregular occupancy before the
18--26 metre cross-fade. The authoritative grass-side mask uses a broader
nonlinear feather, and surviving boundary blades shorten with coverage, so
dirt and leaf-litter transitions do not terminate as a same-height wall. This
composition changes no mesh topology, entity, material, texture fetch, or draw
count. Beyond the geometric range, the
terrain material retains only the band-limited aggregate colour and normal
response of the sward. Bevy's standard mesh path supplies WebGPU-compatible
GPU preprocessing, culling, and indirect batches when the adapter supports
them, with its normal fallback on more limited browser devices.
Forest-floor scatter retains its authoritative leaf-litter placement, patch
count, and shared-mesh instancing. The four leaf meshes, three twig meshes,
and their two materials are cached across scene loads rather than rebuilt per
presentation. Detailed leaves now end at 35 metres, while subpixel twigs end
at 24 metres: the
current visibility architecture cannot substitute a cheaper mesh without an
extra entity/draw, and rendering alpha-tested cambered plates to 72 metres was
not worth that overdraw. Each 24-leaf shared patch uses deterministic
nine-vertex cambered, gently tilted, curled oak plates arranged into four
loose, shallow layers plus scattered singles;
  every plate is seated by its lowest vertex slightly below the local patch
  plane and has a bounded lift so it cannot become an upright card. The reused
  oak maps are desaturated and multiplied by seeded tan/brown vertex pigment
  for a dry-leaf albedo response rather than a lighting gain. These plates retain the
leaf plates with the production oak opacity, front/back albedo, normal maps,
and a dedicated dry PBR response with zero canopy AO, low transmission, higher
roughness, and lower thickness; fallen-leaf wind is disabled. Each nine-twig shared
patch uses bowed four-ring, five-to-six-sided segments with buried contact
points, near-zero tapered tips, and at most two deterministic short forks.
Litter entities remain `NotShadowCaster`, and the geometry change
adds no entity, material slot, draw call, gameplay collider, or placement rule.
The local player transform drives nearby blade bending on the client only; this
cosmetic interaction is neither replicated nor included in collision or
tactical authority. The immutable environment and weather snapshots control the procedural ground
material, precipitation particles, fog distance, wind drift, and sunlight.
The Earth-atmosphere path keeps top-of-atmosphere solar source energy available
after the Sun crosses the geometric horizon. Bevy's atmospheric transmittance
and visible-disc calculation then suppress direct surface illumination while
retaining directional civil and nautical twilight scattering. The
no-atmosphere fallback retains an explicit zero-below-horizon direct-light
curve because it has no planetary occlusion. Exposure transitions continuously
from nautical twilight to the moon-conditioned night target between -12 and
-18 degrees solar altitude; the physical 0.533-degree solar disc, ACES
tonemapping, natural bloom, and bounded lookup-table atmosphere stay unchanged.
The filtered 64-pixel atmosphere environment map is the full preset's canonical
indirect PBR source. `GlobalAmbientLight` provides the complete altitude-aware
fallback until that map is allocated and a bounded four-frame handoff grace
elapses, or whenever a preset disables it; afterward
only the authored moonless/moonlit visibility floor and a bounded 10,500-cd/m2
unresolved multi-bounce term remain. Bevy therefore no longer adds the full
30,000-cd/m2 isotropic daylight approximation on top of directional atmosphere
IBL, while shaded bark and foliage retain readable outdoor bounce light.
Large vista grids are deliberately not ordinary replicated ECS components. The
server sends each accepted client one immutable, ordered `SceneVistaBundle`;
the client builds seam-sharing LOD rings split into independently frustum-culled
mesh chunks without inner-area overdraw, colliders, or shadows. Each finer
ring geomorphs its outermost sample row onto the next coarser height surface,
with a one-fine-cell inward blend; this removes cracks and T-junction wedges
without adding skirts, overlap, or gameplay geometry. Vista vertices reuse
the ordinary ground palette in linear color space, preserving regional
forest, wetland, cultivation, and water variation instead of assigning one
average color per LOD; the same boundary interval morphs both height and
reflectance. This adds one vertex attribute but no material, texture sample,
draw call, collider, or replicated state per region. Synthetic
review fixtures normalize each vista LOD to the playable terrain's origin
height, preventing coarse one-sided rings from becoming an invisible ceiling
above the player. Production-composition review cameras keep these vistas
visible rather than reserving them for an isolated horizon plate. The dispatcher samples the final continental terrain pack
at the request's authoritative case-site coordinates and character minute,
materializes the validated document atomically, and passes only its path to the
child. A tactical-only workflow may instead supply the identical format with
`--scene-input`, so tactical processes never load the continental pack.
Standalone tactical development defaults to the committed `dense-woodland`
input when no path is supplied; the server has no alternate noise-terrain
fallback.

Collider-bearing rocks remain compact authoritative `RockRecipe` values. The
client's bounded 18-cubed Surface Nets field now applies archetype-specific
faceting, cleavage, chipping, and asymmetric ground-contact flattening while
remaining inside the unchanged conservative spherical proxy. A shared
lithology-parameterized dielectric material applies bounded macro variation
over one CC0, two-metre photographic rock scan. Its matched diffuse,
OpenGL-normal, and packed AO/roughness/metal channels share a
0.5-tile-per-metre triplanar projection, avoiding UV seams while preserving
physical scale across rock recipes. It adds no emissive response, collider
detail, entity, draw call, displacement, or authoritative silhouette change.

Near-field upward-facing tactical ground similarly uses one CC0, two-metre
dirt scan for aligned diffuse, OpenGL-normal, and AO/roughness response. The
world-space top projection is blended beneath authoritative cover and weather
colour, declines on steep faces, and fades from 42 to 96 metres before its
detail becomes subpixel. It never displaces the authoritative heightfield and
adds three bounded texture reads only within the presentation shader.

Night exposure preserves the physical 0.25-lux full-moon directional light
and a distinct dark moonless floor. A risen illuminated Moon lowers the shared
camera EV100 from -0.5 toward -1.25, modeling visual adaptation without any
per-asset emission or brightness multipliers; a below-horizon Moon leaves
exposure unchanged.

The Surface Nets implementation is deliberately private to the tactical client
and is the first bounded volumetric-meshing primitive, not a cave system. Future
overhang, cliff, or cave patches must replicate a compact deterministic field
recipe rather than a canonical render mesh. Their heightfield collar, removed
terrain triangles, server collision representation, ground-query dispatch, and
traversability contract remain unresolved and are not represented by the
current scene schema. In particular, adding those patches must not move mesh
extraction into the dispatcher or tactical server.

A running tactical client receives a private, server-generated 256-bit
reconnect capability after enrollment. The client retains it in process across
WebSocket reconnects on both native and wasm, and presents it with the character
identity. The server binds it to that transient character/session, consumes and
rotates it on every successful rebind, and rejects missing, wrong, or replayed
capabilities; character IDs alone never authorize a rebind. Capabilities are
targeted server events, not replicated components, and expire with the
server-owned grace record. Rebinding moves the replicated root and inventory
relationships/caches to the new connection without writing tactical state to
SpacetimeDB. The server synchronously claims the grace record before queuing
rebind commands, so duplicate same-frame proofs cannot both succeed; a record
at or past its deadline cannot be claimed, and a claimed record cannot expire
under the deferred rebind. Explicit terminal resolution and grace expiry retain
their normal strategic teardown behavior.

The SpacetimeDB SDK connection is asynchronous: constructing it does not mean
the server identity has arrived. The child pumps that connection until the
identity is available, then installs reducer subscriptions and opens the
tactical WebSocket listener. Bot creation and player joins use that cached
ready identity and never call the SDK's panicking pre-handshake accessor.

Tactical combatants carry an explicit transient Party or Enemy allegiance,
independent of whether a client or the server controls them. Temporary mission
characters are Enemy melee AI and connected adventurers are Party. Offensive
AI deterministically selects the nearest opposing combatant, turns and pursues
in a straight line through normal controller input, stops at the same shared
body-and-arms plus weapon interaction range used by client hit detection, then
uses a server-owned windup and cooldown to enter the same internal
melee-resolution seam as client requests. This direct pursuit intentionally has
no pathfinding yet.

The client sends windup start and completion through one mapped ordered melee
protocol. The tactical server validates melee allegiance, state, range, a fresh
observed windup and cooldown, then authoritative physics line of sight before resolving an
attack. Finite client-reported precision remains trusted because reconstructing
animation and secondary physics is intentionally outside the headless server.
The ordered wire messages are payload enums rather than phase-tagged field
bags: starts cannot carry completion data, melee completions always name a
target, and ranged completion distinguishes a miss from a targeted hit. Raw
finite precision becomes `ReportedPrecision` without clamping or geometric
reconstruction, and duration-backed authority types gate mutation.
Accepted results mutate replicated limb health plus transient blood loss and
imbalance. Shared autoresolve rules derive pain, blood-loss, and imbalance
incapacitation and recover balance over time. Tactical enrollment projects
authoritative body weight, current/maximum blood, and strategic condition
contributions. It preserves the source values for fear, fatigue, hunger,
thirst, and temperature so the client can present the same segmented condition
language as the strategic UI; the same shared derivation as autoresolve
excludes pain and blood from starting incapacitation before recomputing them
live. Actors currently
over the threshold stop moving, attacking, defending, and participating in
offensive AI target selection; imbalance-only incapacitation can recover.
The numeric incapacitation value is the sole stored readiness authority;
active, staggered, and incapacitated status are mechanically derived from it
rather than synchronized through a second boolean or ECS marker.

These per-tick effects remain in memory only. A mission enemy's first transition
into incapacitation counts as its defeat; recovery and later incapacitation do
not count it again. Once all required enemies are defeated, the tactical server
immediately reports `Defeated`. Once every loaded Party combatant is
incapacitated, it immediately reports `Failed`; simultaneous defeat also fails
deterministically. Strategic authority binds the expected living Party count
into the request and active server records, and the trusted dispatcher passes
it to the child. Resolution waits until every expected adventurer has loaded at
least once, no player is still loading, and all required enemies have loaded.
Enrollment is then sealed. Once enrollment has begun, an empty Party has a
ten-second reconnection grace before `Failed`, including when every client
disconnects before the seal. A timeout-disabled development server where nobody
ever joins remains available. Terminal submission retries a frozen result after
synchronous errors no more than once per second, before reevaluating combat
predicates. Queueing is not commitment: only an `end_tactical_server_then`
callback confirming reducer acceptance latches the result and broadcasts the authoritative
Victory/Defeat presentation event, keeps the transport alive for a bounded
three-second display window, and then exits. The delay is strictly post-commit:
it cannot defer strategic authority or create a second outcome. A configured
timeout remains a bounded `Failed` fallback.
Enrollment and terminal progress are private lifecycle enums. A resolution and
its bounded receipt form one frozen value whose retry time, acknowledgement
deadline, transport failure, committed presentation, and finished state exist
only in their applicable variants.

The terminal call carries a bounded authenticated consequence receipt frozen
with the resolution. It contains only Party character IDs, applied (clamped)
limb injuries, incremental blood loss, ammunition use, and bounded equipment
contacts with durable inventory provenance. Strategic authority validates
membership, server enrollment, custody, uniqueness, finite numeric ranges, and
record caps before transactionally applying injuries, blood loss, capability,
filth, ammunition, equipment wear, and defeat morale. Invalid receipts reject
the whole terminal transaction and remain retryable; tactical data cannot
choose outcomes, capture subjects, rewards, or XP. Positions, ticks, and enemy
state remain transient. Player-fired ranged attacks use an ordered intent,
server-validated weapon/range/line-of-sight/timing gate, and authoritative
transient arrow consumption. Their bounded Party ammunition use is populated
in the terminal receipt. Hit precision is the deliberate exception: the server
rejects non-finite values but trusts finite client reports because authoritative
skeletal animation and secondary physics are outside the headless simulation.
Server-owned offensive AI uses the same internal ranged windup/completion and
authoritative ammo path, holds a bounded standoff distance while firing, and
falls back to melee behavior when ranged equipment or arrows are unavailable.

Mission, hostile-group, battle, and outcome-source identities are separate.
Tactical success never chooses a case objective, capture subject, contract
state, or reward. See [Quest authority](../strategic/quest-authority.md).

## Authored content

Repository-authored organizations, dialogue, quests, investigations, and
bestiary records are validated and embedded during their owning crate's build.
Production services deserialize the compiled catalogs at startup; they do not
interpret loose deployment YAML. Stable IDs cross persistence boundaries while
display names remain presentation.

Generated cases retain the catalog revision and deterministic context used to
create them. Canonical truth, weights, hidden evidence thresholds, witness
reliability, and generation traces remain private. See
[Quest generation and investigation](../strategic/quest-generation-and-investigation.md).

## World compilation

Raw historical and geographic sources are native build inputs, not database
tables. `adventuresim-world-import` parses source-specific formats and compiles
them through typed stages into the dependency-light
`adventuresim-world-schema` model. The current canonical artifact uses world
schema 25 and inference rules 9.

The playable region, spatial grid, source identities, inference versions, and
compiled records contribute to artifact identity. Import is resumable only for
the same artifact. The local `just load-world` workflow therefore explicitly
reset-publishes its selected loopback `adventuresim-*` database before loading
the pinned artifact; all existing data in that database is disposable and
discarded.

Normal development consumes the separately pinned compiled runtime bundle:

- `world-1544.json`;
- the schema-5 strategic map manifest and AVIF tile pack;
- the final schema-6 terrain-routing pack;
- generated licensing and source notices.

The strategic map is presentation data served from immutable file-backed
artifacts. Dynamic settlement, party, quest, and route overlays remain
server-rendered. The terrain-routing pack is a native strategic planning input;
its raster cells and A* search state are never persisted in SpacetimeDB.

Source-specific contracts live in the World Data section of the wiki, beginning
with [Source manifests](world-data/source-manifests.md),
[World-data bundles](world-data/world-data-bundles.md), and
[Viabundus](world-data/viabundus.md).

## Detailed system references

- [Development workflow](developing.md)
- [Quest authority](../strategic/quest-authority.md)
- [Quest generation and investigation](../strategic/quest-generation-and-investigation.md)
- [Bestiary authority](../shared/bestiary.md)
- [Dialogue architecture](../strategic/dialogue.md)
- [Measured inventory](../shared/measured-inventory.md)
- [Physiology](../shared/physiology.md)
- [Organizations](../strategic/organizations.md)
- [Strategic simulation](strategic-simulation.md)
- [Strategic route terrain](world-data/route-terrain.md)

## Unified world actors

All living world actors use the canonical `Character` aggregate. Hostility,
patient status, and other roles belong to `CharacterContextMembership`, never
to a character kind. Exact durable roster IDs cross into autoresolve and
tactical handoff; tactical positions, live HP, damage, AI, and ticks remain
transient and are never persisted to SpacetimeDB.

Generated outbreak patients follow the same rule: the Patient membership
references an existing resident Character and an ordinary infection episode.
Private outbreak authority stores only case linkage and provenance. Shared
resident presence tracks context suppression separately from health
suppression. Leaving a case context restores only the former; ordinary disease
recovery or death authority controls the latter. All NPC availability
consumers share the same authoritative projection and never advertise a dead
or still-ill provider from a stale schedule row.

Tactical equipment switching follows the same persistence boundary. Mission
bootstrap maps durable equipment rows to replicated tactical entities, then
all hand, body-slot, attachment, drop, and pickup changes mutate only that ECS
snapshot. Durable inventory IDs remain server-only, the terminal tactical
receipt contains no final equipment topology, and teardown restores strategic
custody/equipment unchanged. Reconnection to the same live tactical server has
a bounded 30-second, server-owned grace period. The replacement connection
entity receives the existing combat/controller components and every tactical
item relationship, so topology and scene state survive without durable writes.
Grace expiry performs the ordinary strategic leave and discards the abandoned
tactical projection. Starting a new mission projects the durable graph again.
