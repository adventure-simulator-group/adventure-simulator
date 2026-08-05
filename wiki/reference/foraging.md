# Foraging

Willow bark, comfrey, poppy, and sage feed the bounded
[Herbalism](herbalism.md) loop. Their public Poor/Ordinary/Fine catalogue
identities preserve grade through stacking, trade, and transfer without hidden
lot state.

Foraging is a personal strategic activity performed in the acting character's
current authoritative vicinity. It never chooses a nearby biome or creates a
travel excursion. Settlement, exact case-site, and en-route camp coordinates
are supported; active movement, tactical encounters, unresolved strategic
encounters, unlocated characters, and stale locations are rejected.

The browser gateway samples the immutable final terrain pack at that coordinate
and attests the package digest, coordinate, location context, normalized
Plains/Forest/Hills/Wetlands mixture, wet/coast access, and cultivation bit. The reducer
accepts this only from the registered gateway, requires terrain routing schema
6 through gateway contract version 3, re-derives the character's location, and rejects
stale digests or mismatched coordinates/context. A browser never supplies a
trusted cultivated boolean.

For an isolated browser demonstration, select **Herbalism and Foraging Demo**
from the normal development scenario roster. Its character and solo party are
durably seeded together at Foraging Demo Woods, an empirically sampled
uncultivated deep-woods cell in the final terrain pack, so the integrated Terrain
skill action opens a real foraging form without one-off spawn controls.

## Resolution

The dialog exposes exactly five stable source categories, in order: High Game,
Low Game, Fish, Harmful Beasts, and Plants. The year-round resource catalog is
in `adventuresim_core::foraging::FORAGE_RESOURCES`; every resource belongs to
one category. Seasonality is explicitly
deferred until a strategic season model exists. Processed charcoal, vinegar,
oatmeal, and rosewater and non-plant honey are not forage targets.

Searches last 1–24 whole hours. Rarity supplies a price-independent base
discovery rate. The acting character's local weighted Terrain check can add at
most 50% to discovery and yield. Multiple targets divide one time budget.
Discovery frequency scales with the fraction of local terrain matching the
resource's habitat; a trace of forest no longer grants full forest output, but
a successful find retains the resource's ordinary per-discovery yield.
Wet-ground and coast attestations count as complete matching microhabitats.
Only the routing pack's authoritative wetland coverage contributes Wetlands
skill weight. The broader river-or-wet-ground microhabitat boolean remains a
resource-availability signal and is intentionally not converted into an
invented wetland area fraction.
Food alone receives a 1.75× subsistence calibration: an eight-hour low-skill
search for the best food in an ideal habitat averages roughly the 2,000 kcal
spent during that interval. Medicinal rarity is unchanged.
Sources are canonicalized before resolution. One search budget is divided
first among selected categories and then among the locally available resources
inside each category. Adding more plant resources therefore cannot manufacture
more search time. The reducer chooses and privately persists unpredictable
entropy. Each search has an immutable, request-keyed private receipt and a
per-character attempt generation, so an exact retry is inert while a request
collision or stale generation is rejected without consulting mutable character
or ecology state. A completed search may find nothing.

Every concrete food or herbal ingredient yield uses validated individual
food-lot creation, retaining catalog mass, calories, value, contamination,
stable physical-object identity, and material revision. Private harvest
provenance binds each output row, object, and lot to its request and exact
canonical place. Yields are never truncated for carrying capacity; ordinary
encumbrance consequences apply afterward.

Actual elapsed time is clipped once at the existing injury/disease boundary.
That elapsed time is conserved over concrete Plains, Forest, Hills, and
Wetlands training
according to the normalized local mixture. Terrain has no stored parent value,
and settlement illegality does not turn training into Urban.
Checks and training use the same aptitude-capped, injury-adjusted path as
travel. Foraging never adds raw attributes to a check, never stores correlated
hours, and routes rejected above-cap training into mastery enjoyment.

## Legality

Foraging is illegal at a settlement or in a cultivated square. High Game, Low
Game, Fish, and Plants also require a license granted by the character's
currently presented, active, dues-current profession; Harmful Beasts requires
none. The Lodge of the Hart King grants the common licenses at every rank,
while High Game is reserved to Master rank. Licenses are global for now
and deliberately ignore local recognition and political boundaries.

Unlicensed sources remain selectable as poaching. A completed illegal search
makes one deterministic Stealth check, even when several selected sources are
unlicensed or settlement/cultivated illegality also applies. Cultivated ground
and otherwise-legal wilderness poaching start
at DC 1.75; settlement exposure starts at DC 2.50; the worse base applies, plus
0.075 per hour after the first, capped at 4.50. Failure subtracts exactly 1.0
as local Infamy; success avoids the reputation event.
An interrupted search creates no partial yield. If illegal work consumed any
time, it still makes exactly one exposure check using the actual elapsed
duration and applies the same local Infamy consequence on failure.

The reducer accepts stable category IDs rather than item IDs and retains those
selected source IDs in its immutable private receipt. The gateway-only
projection omits seed, coordinates, context, terrain, DC, roll, material
identities/revisions, and direct reputation authority, and exposes only the
exact opaque request's player-safe result.
