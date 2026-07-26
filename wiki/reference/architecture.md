# Architecture

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

The current combat prototype calculates attacks but does not yet apply tactical
damage, so no client-controlled path can emit authoritative enemy deaths.
Required-kill missions therefore fail closed unless the server's future combat
pipeline emits the internal authoritative death event.

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
the same artifact; loading a different completed artifact requires an
explicitly reviewed reset of an isolated or otherwise disposable database.

Normal development consumes the separately pinned compiled runtime bundle:

- `world-1544.json`;
- the schema-5 strategic map manifest and AVIF tile pack;
- the final schema-5 terrain-routing pack;
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
