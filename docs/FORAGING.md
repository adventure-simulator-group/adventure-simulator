# Foraging

Foraging is a personal strategic activity performed in the acting character's
current authoritative vicinity. It never chooses a nearby biome or creates a
travel excursion. Settlement, exact case-site, and en-route camp coordinates
are supported; active movement, tactical encounters, unresolved strategic
encounters, unlocated characters, and stale locations are rejected.

The browser gateway samples the immutable final terrain pack at that coordinate
and attests the package digest, coordinate, location context, normalized
Plains/Forest/Hills mixture, wet/coast access, and cultivation bit. The reducer
accepts this only from the registered gateway, requires terrain schema 5 through
gateway contract version 2, re-derives the character's location, and rejects
stale digests or mismatched coordinates/context. A browser never supplies a
trusted cultivated boolean.

## Resolution

The year-round resource catalog is in
`adventuresim_core::foraging::FORAGE_RESOURCES`. Seasonality is explicitly
deferred until a strategic season model exists. Processed charcoal, vinegar,
oatmeal, and rosewater and non-plant honey are not forage targets.

Searches last 1–24 whole hours. Rarity supplies a price-independent base
discovery rate. The acting character's local weighted Terrain check can add at
most 50% to discovery and yield. Multiple targets divide one time budget.
Discovery frequency scales with the fraction of local terrain matching the
resource's habitat; a trace of forest no longer grants full forest output, but
a successful find retains the resource's ordinary per-discovery yield.
Wet-ground and coast attestations count as complete matching microhabitats.
Food alone receives a 1.75× subsistence calibration: an eight-hour low-skill
search for the best food in an ideal habitat averages roughly the 2,000 kcal
spent during that interval. Medicinal rarity is unchanged.
Targets are canonicalized before resolution. The reducer chooses and privately
persists unpredictable entropy; once that seed is chosen, replay from the
private authority is deterministic. A completed search may find nothing.

Food uses validated individual food-lot creation, retaining catalog mass,
calories, value, and contamination. Herbalist ingredients use validated
fungible stacks. Yields are never truncated for carrying capacity; ordinary
encumbrance consequences apply afterward.

Actual elapsed time is clipped once at the existing injury/disease boundary.
That elapsed time is conserved over concrete Plains, Forest, and Hills training
according to the normalized local mixture. Terrain has no stored parent value,
and settlement illegality does not turn training into Urban.
Checks and training use the same aptitude-capped, injury-adjusted path as
travel. Foraging never adds raw attributes to a check, never stores correlated
hours, and routes rejected above-cap training into mastery enjoyment.

## Legality

Foraging is illegal at a settlement or in a cultivated square. A completed
illegal search makes one deterministic Stealth check. Cultivated ground starts
at DC 1.75; settlement exposure starts at DC 2.50; the worse base applies, plus
0.075 per hour after the first, capped at 4.50. Failure subtracts exactly 1.0
from `CharacterVirtue`; success avoids the loss. This never changes notoriety.
An interrupted search creates no partial yield. If illegal work consumed any
time, it still makes exactly one exposure check using the actual elapsed
duration and applies the same Virtue consequence on failure.

The reducer retains one private replay authority row per character. The
gateway-only projection omits seed, coordinates, context, DC, roll, and direct
Virtue state, and exposes only the exact opaque request's player-safe result.
