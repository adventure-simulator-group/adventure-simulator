# Architecture

Herbal preparation is a strategic reducer. It persists catalogue-backed
ingredient/remedy inventory rows and trained Herbalism hours only; tactical
positions, damage, HP, enemies, and other tactical tick state remain transient.

Static item definitions cross the strategic boundary as a build-time embedded
catalog. YAML is authoring input only; the flattened SpacetimeDB `Item` table
is its deterministic persistence/client projection. Inventory ownership,
custody, amount, and condition remain strategic state and are not definition
content. See [Item definition authoring](item-authoring.md).

Adventure Simulator separates persistent strategic play from transient
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
- inventory, equipment condition, currency, and custody;
- settlements, organizations, journeys, cases, contracts, and investigations;
- mission requests, authenticated outcome receipts, final battle results, and
  finalized loot;
- compiled world identity, source manifests, and strategic route facts.

Tactical servers keep positions, movement, enemies, attacks, per-tick damage,
temporary health, physics state, and scene entities in memory. Those values are
not mirrored into SpacetimeDB. A tactical session may report an authenticated
terminal result, but strategic authority decides which durable consequences
that result is allowed to create.

Random encounters are durable strategic journey events only at their
boundaries: timing, route position, available choices, and final outcome may be
persistent, while the combat simulation remains transient.

## Strategic authority

The SpacetimeDB module owns authoritative mutations. Reducers validate the
current party, character, location, time, custody, and source identity before
changing state. Shared-core functions perform deterministic calculations, but
do not grant authority by themselves.

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
- a selected-character cookie chooses presentation context but is not yet a
  complete player-to-character authorization model;
- non-loopback use therefore requires the explicit insecure-development
  opt-in until account ownership is implemented;
- tactical dispatch claims are one-use, digest-bound capabilities rather than
  copies of the gateway token.

Observer-specific truth remains private. Browsers receive only gateway-filtered
views of contracts, dialogue, investigations, evidence, physiology, and
case-site knowledge. A public or subscribed row is not automatically safe to
use as an authorization decision.

The gateway settlement-NPC roster is likewise an explicit player-visible
projection rather than the authoritative population row. It includes stable
identity, home settlement, visible description, occupation, household, local
role, service, and conversation routing. It omits private demographic sex and
the internal projection traversal key. Browser quest discovery builds its
candidate and commitment from visible age, presentation, profession, role, and
presence. Presentation is committed as seen but is not interpreted as private
sex; the public developer flow leaves that selector empty.

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
Confirmed or selected character IDs are remembered in a bounded,
browser-scoped roster cookie. The strategic header resolves only remembered,
non-temporary rows into its character switcher. Both the roster and
selected-character cookies are local selectors, not authentication or
ownership proof.

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
Accepted results mutate replicated limb health plus transient blood loss and
imbalance. Shared autoresolve rules derive pain, blood-loss, and imbalance
incapacitation and recover balance over time. Tactical enrollment projects
authoritative body weight, current/maximum blood, and strategic condition
contributions; the same shared derivation as autoresolve excludes pain and blood
from starting incapacitation before recomputing them live. Actors currently
over the threshold stop moving, attacking, defending, and participating in
offensive AI target selection; imbalance-only incapacitation can recover.

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
predicates, and exits only after successful submission. A configured timeout
remains a bounded `Failed` fallback.

The strategic reducer selects and commits the durable outcome; the tactical
server's XP argument is deliberately ignored. Tactical combat still does not
persist strategic wounds or create an authenticated combat receipt. Ranged AI
and durable combat consequences also remain unimplemented.

Mission, hostile-group, battle, and outcome-source identities are separate.
Tactical success never chooses a case objective, capture subject, contract
state, or reward. See [Quest authority](quest-authority.md).

## Authored content

Repository-authored organizations, dialogue, quests, investigations, and
bestiary records are validated and embedded during their owning crate's build.
Production services deserialize the compiled catalogs at startup; they do not
interpret loose deployment YAML. Stable IDs cross persistence boundaries while
display names remain presentation.

Generated cases retain the catalog revision and deterministic context used to
create them. Canonical truth, weights, hidden evidence thresholds, witness
reliability, and generation traces remain private. See
[Quest generation and investigation](quest-generation-and-investigation.md).

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
with [Source manifests](source-manifests.md),
[World-data bundles](world-data-bundles.md), and
[Viabundus](viabundus.md).

## Detailed system references

- [Development workflow](developing.md)
- [Quest authority](quest-authority.md)
- [Quest generation and investigation](quest-generation-and-investigation.md)
- [Bestiary authority](bestiary.md)
- [Dialogue architecture](dialogue.md)
- [Measured inventory](measured-inventory.md)
- [Physiology](physiology.md)
- [Organizations](organizations.md)
- [Strategic simulation](strategic-simulation.md)
- [Strategic route terrain](route-terrain.md)
