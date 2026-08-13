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
upsampled and receive bounded seeded microrelief before the tactical server
builds its collider and replicates the heightfield. Each client builds the
render mesh from those same samples, so presentation, movement, and ground
queries share one authoritative surface without sending mesh data. The tactical server owns the playable collider; coarse vista
data is presentation-only and is not tactical tick state or SpacetimeDB state.
Generated tree trunks and rocks are server-authoritative static movement
obstacles. Trees replicate a compact kind and transform; rocks additionally
replicate a seed, archetype, lithology, dimensions, and conservative collision
radius. The server creates only static primitive colliders. Each client samples
the recipe and extracts a low-poly Surface Nets mesh constrained inside the
collider; no render mesh is generated or transmitted by the server. Trees use a seeded
four-order branch skeleton and five smoothly cross-faded presentation levels:
real tapered wood with individual veined leaf cards, leafed-twig impostors,
small-branch impostors, crown-branch impostors, and finally one camera-facing
whole-tree billboard. Each successive level collapses exactly one botanical
order. Every aggregate card derives its position and extent from the actual
seeded descendant twigs, so crown mass, asymmetry, and gaps remain recognizable
through the transitions while retaining parallax longer than a direct
crown-to-billboard swap.
The individual-leaf crown uses an 8-triangle cambered PBR card and a 30-triangle
terminal bud. Once that camber falls below useful screen size, each leaf
cross-fades to a two-triangle flat PBR
card before terminal shoots collapse into twig cards. Both leaf stages use the
same generated oak front/back albedo, DirectX normal maps, AO/roughness, and opacity mask;
the cambered card supplies close depth and foreshortening while the mask preserves one
lobed silhouette throughout the transition. Both stages retain the same
biological attachment, two-sided shading, per-leaf shade variation, and
vertex-shader wind phase. Seven stable
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

For rendering, the client deterministically expands that coarse categorical
map and displaces its lookup boundary with two scales of smooth noise. The
shader still selects exactly one cover material at every fragment—there is no
colour-gradient blend—but boundaries such as forest floor against grass no
longer expose square source-grid cells.

Generated tree crowns stamp leaf-litter cover into this grid. Grass macro
patches conservatively reject any footprint intersecting those cells. Separate
shared meshes scatter a dense dry-leaf carpet and a looser layer of longer
twigs with independent deterministic placement over the warmer forest-floor
material. Open soil remains tall grass, wet ground selects reeds,
and sufficiently hilly samples select loose stone; these profiles are mutually
exclusive at a location even when one profile renders several compatible
details. Loose-stone cells deterministically place non-colliding, separately
shaded rock instances generated from four shared client-side volumetric meshes;
they do not enter the foliage wind or player-bending shader.

The bounded client Surface Nets extractor is reusable infrastructure for future
sparse volumetric terrain patches, but this iteration does not define a cave,
cliff, overhang, heightfield-collar, or traversability schema. Those patches
must eventually replicate compact deterministic field recipes rather than
meshes. Their server collision and ground-query contract must be designed
without moving render-mesh extraction into the dispatcher or tactical server.

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
blade geometry. Regional vista vertices retain the same environmental samples,
including an aggregate sward-coverage channel, so open terrain continues the
grass response through every vista ring. Vista slopes use continuous
height-gradient normals instead of per-cell face normals; sufficiently exposed
hilly samples reuse the generated two-color rock surface through coarse-safe triplanar
sampling. The locally controlled player's position and velocity flatten
and push nearby grass as a presentation-only effect.

Ordinary temperate understory shrubs use one shared procedural common-hazel
(`Corylus avellana`) specimen rather than a unique mesh per scatter point. Its
multi-stem architecture and alternate broad leaves come from the same
parameterized woody-plant generator used for the English oak, with shrub-scale
height, crown, stem-count, shoot, and leaf parameters. Cambered near leaves and
flat alpha-card far leaves share one generated, palette-constrained
albedo/opacity/normal/AO/roughness material and
the existing tree-leaf wind shader.
Root self-shadow and a darker centre rib are occlusion responses; a small
solid-color palette and softened upward normals keep the dense field readable
without making individual cards look heavily lit. A procedural terrain
material selects hard-bounded forest floor, dry ground, mud, cultivation,
stone, water, and snow albedo regions while retaining small-scale normal and
AO variation. Tree sampling follows canopy
coverage; rock sampling uses an independent deterministic roll scaled by hilly
coverage, so the two features do not suppress one another. Weather affects ground
wetness/snow tint, bounded rain or snow particles, wind drift, sunlight
transmission, and distance fog. Coarse vista samples preserve local peaks and
render as seam-sharing rings of independently culled mesh chunks out to 50 km;
coarser rings leave the playable and finer-ring interiors open rather than
overdrawing them. The 50-metre and 250-metre regional rings also
deterministically scatter bounded samples of the production whole-tree
impostor over canopy-bearing cells; the outer ring relies on aggregate canopy
colour. Those presentation-only trees share one cached atlas family, have no
gameplay collider, and are seated on the same morphed vista surface as the
terrain.

Near-tree PBR leaf cards participate in the horizon-aware directional shadow
map, producing both cast shadows and leaf-on-leaf self shadow. Their vertex data
also carries a deterministic canopy-visibility term that darkens protected
interior foliage without relying on native screen-space ambient occlusion,
which is unavailable on WebGPU. The same alpha cutoff and CPU-synchronized wind
phase are used by forward, depth/prepass, and shadow passes so moving leaf
silhouettes remain aligned. Aggregate distant tree LODs do not cast individual
leaf shadows.
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
