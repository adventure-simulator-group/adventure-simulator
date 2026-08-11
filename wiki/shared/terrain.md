# Map

## Weather overlays

The imported Plains, Forest, Hills, Wetlands, and Urban mixture remains an
immutable normalized five-member description of the land. Strategic weather
does not rewrite that source data. Rain derives a temporary effective mixture
at journey departure: cells below 100/1000 Wetlands absorb rain without
becoming wetlands, while susceptible cells gain Wetlands and deterministically
renormalize the displaced underlying weights. A separate bounded saturation
value slows travel through mud and waterlogging, including terrain already at
100% Wetlands.

Snow cover is a separate overlay, never a sixth biome weight. It blends Snow
expertise with the ordinary terrain check. Cover splits the road-discounted
training budget between Snow and the underlying biome rather than duplicating
exposure.

The terrain profile affects strategic routing and can select tactical scene
context. Tactical scene generation now has a shared versioned input boundary:
production sampling and named synthetic fixtures provide the same playable
heights, environmental coverage, immutable weather, and multi-LOD vista
samples. Inputs coarser than two metres are deterministically bilinearly
upsampled and receive bounded seeded microrelief before the tactical mesh and
collider are built, so presentation, movement, and ground queries share one
authoritative surface. The tactical server owns the playable collider; coarse vista
data is presentation-only and is not tactical tick state or SpacetimeDB state.
Generated tree trunks and rocks are server-authoritative static movement
obstacles. Their compact kind and transform are replicated so clients render a
matching proxy instead of an invisible collider. Clients present deformed
low-poly rock meshes constrained inside those colliders. Trees use a seeded
four-order branch skeleton and five smoothly cross-faded presentation levels:
real tapered wood with individual veined leaf cards, leafed-twig impostors,
small-branch impostors, crown-branch impostors, and finally one camera-facing
whole-tree billboard. Each successive level collapses exactly one botanical
order. Every aggregate card derives its position and extent from the actual
seeded descendant twigs, so crown mass, asymmetry, and gaps remain recognizable
through the transitions while retaining parallax longer than a direct
crown-to-billboard swap.
The individual-leaf crown uses a 46-triangle lobed and cambered leaf (down from
122 triangles) and a 30-triangle terminal bud (down from 72). Seven stable
primary scaffold clusters each carry their own individual leaves, terminal
buds, progressively simplified wood, leafed-twig cards, small-branch cards,
and crown cards. Projected screen size is evaluated from the active camera's
field of view and viewport height against each cluster's own bounds, so the far
side of a nearby crown may collapse before the near side. Matched dither bands
cross-fade adjacent representations without a whole-tree topology pop; only the
final distant billboard is selected for the entire tree. Generated variants
reuse cached mesh and material handles, allowing Bevy's WebGPU renderer to
instance repeated trees automatically. Leaf wind is evaluated in the vertex
shader with fixed petiole roots, spatially varied gusts, and high-frequency tip
flutter; no per-frame CPU deformation or non-WebGPU feature is required. The
open-grown reference LOD0 is capped by test at 3.6 million triangles, versus
roughly 9.1 million before leaf and bud retopology.
The scene's world-data canopy coverage also shapes the source skeleton
continuously: sparse coverage yields low, wide open-grown trees, while dense
coverage yields taller trees whose first scaffold branches sit above the clear
bole. This value is part of deterministic generation rather than a query over
spawned neighbors, preserving parallel tree construction.

Playable terrain also carries a replicated, authoritative `SceneGround` grid
aligned with its height samples. Each location has one semantic substrate, one
mutually exclusive cover profile, bounded cover density, and cover height. The
tactical server retains the CPU grid and exposes world-position queries; this
is the future boundary for concealment, footsteps, tracks, and traversal, but
those gameplay consumers are not implemented yet. The client uploads a packed
copy as the terrain material map and uses the same values for deterministic
scatter. Texture asset identifiers are presentation details and never become
server authority.

Generated tree crowns stamp leaf-litter cover into this grid. Grass macro
patches conservatively reject any footprint intersecting those cells, while a
shared proof-of-concept mesh scatters dry leaves and twigs over the warmer
forest-floor material. Open soil remains tall grass, wet ground selects reeds,
and sufficiently hilly samples select loose stone; these profiles are mutually
exclusive at a location even when one profile renders several compatible
details.

Grass, shrubs, reeds, leaves, and twigs are deterministic shared-mesh foliage with no gameplay
collider. Grass uses overlapping 3.2-metre shared macro patches containing 729 individually
oriented ribbons: each nearby blade samples a cubic longitudinal curve with
fifteen vertices, then cross-fades beyond 34 metres to a stable 144-blade
subset using seven vertices per ribbon. The retained blades widen by the square
root of the density ratio, preserving aggregate coverage while eliminating
the rejected blades before vertex shading. Internal blade spacing matches the
former one-metre patch, so the larger footprint cuts grass render entities by
roughly an order of magnitude without reducing near-field density. Canopy,
water, cultivation, and snow select stable blades within the shared mesh rather
than opening macro-patch-sized holes.
Both representations evaluate the same authored lean, layered spatial wind,
and player displacement curve, and an edge-on ribbon turns partially toward
the view so it retains useful screen width without becoming a full billboard.
The geometric sward ends by 132 metres; beyond it, band-limited terrain colour
and normal variation carries the far-field grass response without sub-pixel
blade geometry. The locally controlled player's position and velocity flatten
and push nearby grass as a presentation-only effect.
Root self-shadow, broad colour variation, a darker centre rib, and softened
upward normals keep the dense field readable without making individual cards
look heavily lit. A procedural terrain material blends forest floor, dry
ground, mud, cultivation, stone, wetness, and snow while adding small-scale
colour and normal variation. Tree sampling follows canopy
coverage; rock sampling uses an independent deterministic roll scaled by hilly
coverage, so the two features do not suppress one another. Weather affects ground
wetness/snow tint, bounded rain or snow particles, wind drift, sunlight
transmission, and distance fog. Coarse vista samples preserve local peaks and
render as seam-sharing rings of independently culled mesh chunks out to 50 km;
coarser rings leave the playable and finer-ring interiors open rather than
overdrawing them.
Hazard mechanics remain future work; [#212](https://github.com/adventure-simulator-group/fabelgeist/issues/212)
tracks that handoff and committed-result contract.
The world map is a grid where each square has a height and an enum for the terrain type. Each of these affect both the speed of travel and the difficulty of climb/swim check to avoid injury. We should not try and create our own, we should be able to find both height and biome data from some open GIS dataset. At minimum, it should be easy to find modern data for these, but there may also be a historical dataset that we can use.

The strategic import records a bounded terrain profile on each road/ferry edge:
elevation, ascent/descent, grade, slope/aspect, roughness, relief, landforms,
water adjacency, and versioned seasonal/encounter tags. Because the historical
road source has endpoint topology but no polyline, these facts explicitly use
straight endpoint geometry. They select strategic travel and possible scenes;
a tactical server still owns all live terrain interaction.

## Height
Traveling from a lower height to a higher one may require the characters to make a climb check (based on upper body strength vs weight) based on the slope, and traveling from higher to lower may require the characters to make an agility check.

## Terrain Type
As this is an enum, we can store extra information in each variant. For example, the "River" variant could include its depth and velocity, both of which would contribute to how hazardous it is to ford.

In addition to the costs imposed to traveling, it may also affect stealth and detection. Forest cover may largely prevent detection from flying enemies but also make it much easier for an enemy to ambush you, presenting a trade-off of risk which you might assess based on which enemies are known to exist in an area and which you are better able to defend against.

## Pathfinding
As described in the [travel](../strategic/travel.md) page.

# Weather
This probably isn't worth putting in the MVP, especially for such a temperate place like Italy, but eventually there should be a weather map that affects the way that you travel though terrain. Heavy snow would slow down land travel, frozen lakes and rivers become possible to cross without swimming (but also risky if the ice is thin), and rain makes climbing very difficult.

# Points of Interest
For the MVP, there is no need for any point of interest other than enemy camps/lairs/nests relevant to active [quests](../strategic/quests.md) in an area as well as [settlements](../strategic/settlement.md). The former are simply placed randomly (though not too close to any other point of interest), the latter should be obtained from a GIS dataset. If we can't find *historical* GIS data on Italian settlements then we can just use modern data and rely on [Cunningham's Law](https://meta.wikimedia.org../Cunningham%27s_Law) to fix it.

# Underground
In our setting, there is ostensibly a vast underground network of caves, crypts, tunnels, Ratling under-cities, Dwarven strongholds, and even antediluvian ruins. But this sounds hard, therefore we shouldn't bother with it for the MVP. All of the quests will conveniently take you to overland locations, which don't even need to have structures. 
> Halbe: You are essentially being hired by local municipalities to clear out homeless encampments. If only we had this in the IRL modern setting...
## Foraging

Personal foraging trains only the Plains, Forest, and Hills leaf skills in the
normalized mixture of the character's current 1 km vicinity. The Terrain
heading remains a presentation aggregate and is never awarded or stored.
Cultivated ground affects legality, not the biome mixture. High Game, Low Game,
Fish, Harmful Beasts, and Plants divide one selected search-time budget; source
availability follows the local habitat rather than selecting another vicinity.
