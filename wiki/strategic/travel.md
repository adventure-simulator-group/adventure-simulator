# Travel

Travel is a strategic activity. The party plans a route, chooses a daily
schedule, prepares supplies, and advances time until it reaches the destination,
stops at camp, or is interrupted.

## Departure weather

Strategic weather is evaluated from absolute minute, coarse geographic cell,
and elevation using a versioned deterministic authority. Advecting
synoptic-scale fields provide spatially and temporally correlated temperature,
humidity, dew point, pressure, wind direction and speed, shear, instability,
and broad lift. Those conditions diagnose low, middle, and high cloud decks;
sufficiently moist ascending nimbostratus or cumulonimbus produces bounded rain
or snow. Recent intervals deterministically produce ground moisture and snow
cover. The authority reconstructs these conditions from its world seed rather
than storing per-tile weather rows.

Routing snapshots the weather rules version, interval, precipitation,
intensity, moisture, and snow cover at departure. The snapshot participates in
route cache identity and is persisted with route authority. Rain may increase
effective Wetlands before path search and add mud duration. Snow cover blends
the party's Snow expertise into route checks and cost before path search, then
splits (without duplicating) the road-discounted training budget. An active
journey is never rerouted as later intervals pass.

## Route planning

The settlement map shows historical roads, ferries, settlements, known exact
case sites, and the party's current route. Selecting a destination previews the
fastest route available to that party.

Routes account for roads, open ground, woods, hills, water, crossings, and
directional elevation. A party with stronger Terrain skills may prefer a
different path because its members move more efficiently through the terrain
they understand.

The five Terrain leaves are Plains, Forest, Hills, Wetlands, and Urban. Native
wetland coverage is retained beneath roads, so marsh crossings use and train
Wetlands even when the road itself supplies the faster travel surface.

Players should make strategic choices about resources, danger, and timing
rather than manually approximate the shortest geometric line.

The source and artifact contracts are documented separately:

- [Viabundus](../engineering/world-data/viabundus.md) covers the historical road and
  settlement source.
- [Strategic route terrain](../engineering/world-data/route-terrain.md) covers compiled
  elevation, water, terrain, and routing facts.

## Speed

Road travel is fastest. Open ground, sparse woods, deep woods, wetlands, steep
grades, and off-road case sites impose progressively greater costs.

Off-road wetland movement is 0.5 km/h. A road over wetland trains the
underlying Wetlands skill at 10% exposure, the ratio between 0.5 km/h wetland
movement and the road's 5 km/h.

The party moves at the pace of its limiting members after attributes,
encumbrance, fatigue, terrain skill, and route surface are considered.
Overloading one character can therefore slow everyone.

## Daily schedule

The leader chooses how many hours per day the party walks and whether that
window is centered on daytime or nighttime. The remaining hours become camp
and downtime.

A longer walking day reaches the destination sooner but leaves less time to
recover fatigue, treat injuries, cook, train, or perform other camp activity.
A member who cannot recover during camp carries fatigue into the next day.

The route preview shows five synchronized rails:

- food;
- water;
- fatigue;
- terrain;
- day and night.

These rails forecast both movement and camp time. Actual camps already reached
remain part of the journey history even if a later rest changes the remaining
forecast.

## Camps and redirection

A journey longer than its current walking window stops at a persisted camp.
While camped, the party can rest, treat injuries, manage supplies, or redirect
the remaining journey.

Choosing a new endpoint changes the plan from the party's current physical
location; it does not teleport the party or reuse an obsolete straight-line
duration. The leader may also turn back.

If the leader becomes unable to act during an expedition, a ready party member
may direct field rest and an evacuation journey on the party's behalf. This is
a narrow rescue authority: it applies only while the leader is publicly
unready and does not transfer leadership or permit the companion to accept
quests, initiate ordinary travel, or command combat objectives. Incapacitated
members remain with the party, consume ordinary time and supplies, and are
carried along the existing journey rather than abandoned or teleported. Their
body and inventory remain part of the party's burden, while staggered members
contribute reduced carrying capacity and incapacitated members contribute none.

If no living member is actionable, ordinary action authority remains
unavailable. At a coherent persisted off-settlement journey camp only, an alive
authoritative leader may nevertheless attempt supplied passive camp rest when
all condition statuses are known and nobody is critical. Publicly symptomatic
members may convalesce this way. An unresolved encounter or incoherent
party/journey/itinerary forecast holds before rest. This permission models the
party remaining at rest; it cannot be used to continue the journey, choose an
encounter response, pursue a case, manage a contract, or exercise leadership.

## Food and water

Every living traveler consumes food and water over elapsed strategic time.
Shared provisions are used before personal supplies. Settlement departure
fills available water capacity, while field camps rely on what the party
carried or can safely obtain.

Insufficient food or water creates durable need and condition penalties.
Emergency alcohol may provide limited hydration, but its alcohol content caps
the useful water and creates its ordinary drinking effects.

See [Inventory](../shared/inventory.md) for provision storage and
[Food and cooking](../shared/food-and-cooking.md) for authoritative food
behavior.

## Weather and field shelter

Travel, waiting, and rest advance deterministic wetness and signed thermal
strain from the party's authoritative route or location. Rain wets exposed
characters; wetness makes cold more dangerous. Worn padding supplies
insulation, while an outer leather-equivalent resistant layer provides the
current all-or-nothing weatherproofing approximation.

Every deliberate field rest chooses either a bivouac or a tent. A bivouac
requires no item and provides no weather protection. A tent must already be in
shared party inventory; its carried weight therefore affects the journey. It
blocks rain and reduces wind during that rest but creates no heat. Current
route persistence has no ford coordinates, so wetlands do not falsely trigger
immersion; a reusable explicit immersion impulse is reserved for future ford
and river actions.

## Stealth, detection, and interdiction

Strategic stealth determines whether nearby groups detect one another early
enough to choose a response. Party size, member skill, terrain, light, speed,
and the opposing group's perception all matter.

A detected party may be intercepted or forced into a slower, disorganized
state. This prevents the map from becoming a consequence-free race in which a
player can always pass through a hostile group.

## Random encounters

Encounter checks occur at deterministic movement boundaries. Terrain,
daylight, route position, and relevant nearby threats influence what can occur.
Retries and different travel chunk sizes do not reroll the same boundary.

An interruption records its strategic time and route position and offers only
the choices justified by the encounter. If combat begins, the tactical or
autoresolve system owns the immediate fight; the journey retains only its
validated interruption and result.

While the encounter remains unresolved, its exact stopped position is not a
camp and cannot be used for rest, custody, or onward travel. A final successful
resolution promotes that same persisted movement minute to an idempotent
reached camp stop when the party still owns the matching canonical incomplete
journey. Preview refreshes, stale or unresolved encounters, absent journeys,
and journeys already at their destination never create a camp identity.

## Case sites

Travel to an exact known case site may leave the road network and follow native
terrain routing. Knowing or tracking the site is navigation state only: it does
not accept a contract, progress an objective, reveal hidden enemies, or grant a
reward.

After resolving or abandoning the situation, the party plans onward travel
from its actual case-site location.

An activity incident also preserves departure provenance at its exact case
site while a character remains a current member of the incident-owning party
and that party still occupies that same site. This remains true after the
incident is resolved or avoided, so resolving combat or surrendering does not
erase the location before the party crosses back into the settlement. The
resolver uses the existing case-site authority coordinates. It does not
disclose the private incident or make unrelated quest sites exact; ordinary
investigation knowledge remains required everywhere else. A still-pending
matching incident blocks a fresh forage attempt (after immutable retry
handling), even though its site remains recognizable for withdrawal.

When an exact case site is co-located with its destination settlement, returning
to the settlement is an immediate location-boundary transition rather than a
journey. The server still verifies the actor, party, unresolved-encounter,
readiness, and exact current-site authority, then applies the ordinary arrival
state refresh and avoids only the pending incident for that same party and
site. It does not synchronize clocks, consume or refill field supplies, advance
time, create a camp, scan for travel encounters, or accept a caller-supplied
nonzero route for the zero-distance crossing.

## Training and exposure

Walking trains the Terrain skills corresponding to the ground crossed. Road
travel still provides discounted exposure to the underlying terrain; camp time
does not.

The active route stores its validated geometry and terrain mixture so a
character's later training cannot retroactively change a journey already in
progress.

Combat interruptions materialize an exact durable roster of full Characters.
Camp renders those counterparties and permits contact before combat. Contact is
revisioned and idempotent, makes both sides aware, and removes Sneak for the
remainder of that encounter.
