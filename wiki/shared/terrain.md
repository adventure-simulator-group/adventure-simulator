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
heights, environmental coverage, immutable weather, and multi-LOD vista samples.
Inputs coarser than two metres are deterministically bilinearly upsampled and
receive bounded seeded microrelief before the tactical server builds its
collider and replicates the heightfield. Each client builds the render mesh from
those same samples, so presentation, movement, and ground queries share one
authoritative surface without sending mesh data. The tactical server owns the
playable collider; coarse vista data is presentation-only and is not tactical
tick state or SpacetimeDB state. Generated tree trunks and rocks are
server-authoritative static movement obstacles. Trees replicate a compact kind
and transform; rocks additionally replicate a seed, archetype, lithology,
dimensions, and conservative collision radius. The server creates only static
primitive colliders. Each client samples the recipe and extracts a low-poly
Surface Nets mesh constrained inside the collider; no render mesh is generated
or transmitted by the server. Trees use a seeded four-order branch skeleton and
five smoothly cross-faded presentation levels: real tapered wood with individual
veined leaf cards, leafed-twig impostors, small-branch impostors, crown-branch
impostors, and finally one camera-facing whole-tree billboard. Each successive
level collapses exactly one botanical order. Every aggregate card derives its
position and extent from the actual seeded descendant twigs, so crown mass,
asymmetry, and gaps remain recognizable through the transitions while retaining
parallax longer than a direct crown-to-billboard swap. Runtime atlas generation
rasterizes low-sided woody silhouette tubes and progressively sampled,
coverage-compensated leaves rather than rebuilding the multi-million-triangle
production bark and leaf meshes for every card. The directly viewed near trunk,
root flare, and leaf cards remain the full production meshes. Oak bark relief is
evaluated in its near-tree material from three scalar-height projections whose
growth coordinates follow the metric branch sweep. Its packed ambient occlusion
combines a half-resolution broad horizon with full-resolution local cavity
sharpening. Bounded mip-aware near-camera parallax and a three-step
dominant-light horizon add view-dependent cavity depth without altering the mesh
or tactical collider. The complete load-bearing trunk and root system is one
bounded implicit surface, eliminating the unrelated mesh
loops and normal discontinuity of a flare/tube handoff. Swept higher-order
branches duplicate their cylindrical wrap position and normal exactly. Bark
fissure phases vary deterministically per specimen and close into irregular
plates, so no permanent groove advertises that wrap. Intermediate crown wood
excludes the depth-zero root flare because the separately streamed trunk remains
resident, avoiding duplicate geometry and duplicate Surface Nets construction.
Oak terminal shoots occur in separated pulses along each secondary axis. Their
compact leaf flushes form a handful of readable foliage masses with stable
interior windows instead of a uniformly noisy twig lattice. Every aggregate tree
card is baked from this same clustered source geometry, preserving those masses
and gaps across LOD transitions. Unresolved canopy occlusion is bounded and
paired with diffuse leaf transmission so crown interiors remain legible in
WebGPU without a separate subsurface or screen-space effect. The individual-leaf
crown uses an 8-triangle cambered PBR card and a 30-triangle terminal bud. Once
that camber falls below useful screen size, each leaf cross-fades to a
two-triangle flat PBR card before terminal shoots collapse into twig cards. Both
leaf stages use the same generated oak front/back albedo, DirectX normal maps,
AO/roughness, and opacity mask; the cambered card supplies close depth and
foreshortening while the mask preserves one lobed silhouette throughout the
transition. Both stages retain the same biological attachment, two-sided
shading, per-leaf shade variation, and vertex-shader wind phase. Seven stable
primary scaffold clusters each carry their own individual leaves, terminal buds,
progressively simplified wood, leafed-twig cards, small-branch cards, and crown
cards. Projected screen size is evaluated from the active camera's field of view
and viewport height against each cluster's own bounds, so the far side of a
nearby crown may collapse before the near side. Dither bands cross-fade adjacent
representations without a whole-tree topology pop. The cheap aggregate tiers
deliberately overlap their full-strength intervals: the incoming silhouette
fades in before the outgoing, non-identical silhouette fades away, preventing
transition holes. Only the final distant billboard is selected for the entire
tree. Generated variants reuse cached mesh and material handles, allowing Bevy's
WebGPU renderer to instance repeated trees automatically. The cache initially
retains only the deterministic branch/leaf recipe output and shared materials.
Trunks, detailed scaffold meshes, the cambered and flat leaf representations,
aggregate crown wood, and each baked atlas are generated only when the camera's
conservative LOD-residency mask first requests them. The two individual-leaf
meshes coexist only in a widened handoff band; a camera wholly inside either
leaf tier does not construct the other representation merely because it belongs
to the same tree. This is demand generation rather than eviction: once
requested, a variant's handles remain cached for reuse during the scene. Leaf
wind is evaluated in the vertex shader with fixed petiole roots, spatially
varied gusts, and high-frequency tip flutter; no per-frame CPU deformation or
non-WebGPU feature is required. The open-grown reference LOD0 is capped by test
at 3.6 million triangles, versus roughly 9.1 million before leaf and bud
retopology. The scene's world-data canopy coverage also shapes the source
skeleton continuously: sparse coverage yields low, wide open-grown trees, while
dense coverage yields taller trees whose first scaffold branches sit above the
clear bole. This value is part of deterministic generation rather than a query
over spawned neighbors, preserving parallel tree construction.

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
details. Loose-stone cells use low-frequency site fields stretched along the
local fall line to form open ground, sparse margins, and coherent dense scree
trains instead of a uniform pebble carpet. Two density-preserving shared mesh
families retain the same individual stones through hero, near, and
camera-facing billboard LODs. Their candidate lattice is coarse enough to
avoid routine overlap, and their rounded bases extend slightly below the
terrain so grazing views read embedded stones rather than open polygon caps.
These non-colliding separately shaded instances do not enter the foliage wind
or player-bending shader.

Sparse river-bluff patches use compact deterministic recipes rather than meshes.
A bounded classifier tests intended topology and heightfield fitting error: a
resolved steep plane stays on the heightfield, while an unresolved scarp or true
undercut selects the implicit representation. Recipes are capped at 524,288
field samples and 100,000 extracted triangles. The committed cliff retains
approximately 0.28--0.35-metre sampling across the face and crest; the bounded
tile assigns the remaining samples longitudinally. Shared code owns its scalar field, thin face-following
static collision bands, and nearest-surface-below dispatch for the upper and
lower benches. The committed landform is a broad lower floodplain meeting a
broad upper terrace at an asymmetric, concave 28-metre bluff boundary. Its
central exposed sector carries metre-scale face undulation; the crest and face
then taper through long returned shoulders into surrounding terrain instead of
terminating at squared patch edges. A continuous joint-bounded
failure region is a strongly tapering wedge bounded by four landform-scale
piecewise fracture vertices on each diagonally traversing side and a
three-segment angular oblique release base. A few broad blended facet planes
break up the recessed surface without introducing sub-grid grooves. It opens to
a modestly lowered crest and pushes a fresh rock wedge rearward without cutting
a hole through the mass. The client evaluates one continuous implicit terrain
tile from the authored multi-valued bluff solid and solid-below the replicated
authoritative `SceneTerrain`, including the aggregated fan. The authored
contribution is an unbounded-downward, rearward terrace solid: the intersection
of solid-below-crest and solid-behind the curved scarp. It therefore has no
recipe box sides, back, or bottom to expose. Its
40-by-34-metre local footprint extends six metres beyond both face ends, five
metres beyond the fan, and eight metres behind the rock mass, so every outer
edge is ordinary single-valued ground. Surface Nets therefore extracts the face,
crest, lower floodplain, upper terrace, and returned shoulders as one connected
surface rather than filtering a local face out of a finite box. There is no
local collar, crest cap, analytic stitch, finite back wall, or side shell.

The ordinary playable terrain keeps collision and query authority throughout,
while its base and camera-detail render meshes omit only cells strictly inside
the tile rectangle. At the distant perimeter the client projects every open
tile-boundary vertex onto the same authoritative heightfield triangle sampled by
the regular terrain. The regular renderer retains boundary-straddling cells,
producing a narrow coplanar overlap instead of a raster-cell gap. Pending tile
presentation runs after ordinary terrain presentation and waits until
`SceneTerrain` and its base material are present, so replication insertion
order cannot drop the mesh. Its ground-material clone is then synchronized
from the finalized source terrain material whenever presentation refreshes,
preserving every ground map, palette, cover, and far-sward field while changing
only the tile's camera-detail cutout flag. The client partitions the one
extracted topology into disjoint ground and rock triangle sets that reuse the
same positions and normals. Single-valued tile ground therefore uses the exact
ordinary terrain material and ground map; only the exposed scarp uses sandstone
vertex color. The tile-specific ground material preserves the base terrain's
normal and roughness regime while disabling only its camera-local detail cutout;
the camera-detail mesh now uses that same shading regime, so changing geometry
owners cannot reveal the tile rectangle through normal or roughness response.
The implicit tile alone supplies its surface. Ground triangles are
selected from the authored scarp envelope: the face, undercut, and low toe are
rock, while the crest, terrace, returns, and perimeter remain terrain regardless
of coarse heightfield quantization. Client detail-ground vertices and all
terrain-attached scatter placements (grass, understory, litter, and loose
stone) query the same compact patch recipe for the highest single-valued
surface, while cells crossing the scarp are omitted. Capture evidence reports
the number of grass presentation entities seated on the upper terrace and
fails the focused fixture when none are present. This is presentation-only and
does not change heightfield collision authority. Ground cover and substrate remain inherited from the surrounding
terrain rather than forming a rectangular evidence mask. Buntsandstein bedding uses two broad, subtly different
warm-colour strata. Only one upper resistant bed projects by about 0.85 metres
and one broad weaker interbed recesses by about 0.60 metres. Each is a
finite-thickness course with a 0.84-metre full-strength interior and 1.0-metre
continuous edges. The course centers are separated by 2.88 metres, so midpoint
weights fall below five percent and the resistant course forms a real break in
slope rather than broad material blending while retaining smooth extraction
rather than a Gaussian bulge. Their warped, attenuated extrema retain at least
0.85 metres of absolute face-depth separation, so extraction produces a
continuous ledge silhouette rather than a sampled scalloped edge; the localized
toe lip supplies the principal structural projection. Their slowly warped,
laterally coherent profiles ease across the failure plane and fade into the
returned ends rather than switching into short pointed segments. A localized
resistant toe lip projects above a sheared, irregular toe undercut. The
capture harness suppresses foreground grass in its dedicated grazing views
without mutating authoritative ground semantics. The authoritative obstacle
pass separately rejects generated trees throughout the patch's complete
returned-shoulder width and the lower-front through upper-contact depth zone;
this broad oblique-safe exclusion covers both camera-to-target corridors because
ground semantics alone do not control woody obstacle placement. The recess is restricted to an approximately
2.8--4.0-metre-wide shallow feathered sector beneath the lowest resistant bed,
with only a 1.0--1.7-metre-wide core reaching full depth. Its asymmetric roof
varies from roughly 0.45 metres at the flanks to a single local peak below 1.2
metres, and recession depth feathers to a local 0.8-metre maximum. The base
heightmap is replaced by matching local crest heights and benches. It is also
the single-valued component of the client tile scalar, so the tile continues
naturally across the upper terrace, lower floodplain, and returned shoulders
before meeting ordinary ground at its distant perimeter. The heightfield rise
stays tightly buried behind the central implicit scarp, but blends to a
multi-metre, grade-bounded transition before entering the heightfield-owned
returned shoulders. This prevents the coarse grid from exposing a grass-topped
near-step or lateral wall outside the central collision sector. One deterministic,
multi-lobed debris fan in the
authoritative heightfield ties the missing scar volume to the toe without
becoming a continuous skirt. Three separated metre-scale ridges are
height-graded down the fan, with bare soil between them and an explicitly
debris-free undercut flank. It uses ordinary heightfield collision and surface
queries; the client creates no discrete debris entities, rigid bodies,
colliders, or loose-stone scatter patches. The Gaussian tails settle
continuously into the floodplain instead of creating a compact polygonal
material boundary. The apron terminates against the intact resistant lip rather
than following the recessed undercut rearward. Collision bands sample the same
face at finer lateral and vertical intervals, remain only 0.7 metres deep, and
begin behind the most recessed surface in each band. Bands intersecting the
failure scar or localized undercut are omitted, so the authoritative proxy
neither projects in front of the rock nor fills the authored air. The toe
omission is bounded to the recipe's 0.8-metre clearance height; a fitted lip
band begins at that exact elevation, and valid rock above it resumes the
ordinary vertical-gap budget. A band that cannot span a sharp bed or release
joint within the fit tolerance is divided laterally into narrow boxes,
preserving collision on both sides while leaving the discontinuity open.
Implicit collision is limited to the full-height central half of the authored
face. At the same normalized threshold where the crest begins tapering, the
ordinary heightfield collider takes exclusive ownership of both returned
shoulders. Rendering does not change owner at that local transition: the one
continuous implicit tile carries both regions out to the distant, exact
heightfield boundary. There is no pair of overlapping visible surfaces,
exact-line stitch, finite side, or separately shaded rear collar.
Review overlays draw moderately dense thin cyan rectangle outlines for
alternating authoritative proxy bands. Because fitted proxy fronts lie behind
the rendered rock, the review-only outlines are projected onto the corresponding
authored face with a two-centimetre camera bias; they still sample only retained
boxes, so neither the undercut nor failure scar receives cyan fill.
Heightfield-owned returned shoulders require no implicit overlay. The capture
manifest reports central solid-column coverage, maximum vertical and crest gaps,
maximum front offset, and exact undercut clearance alongside the image-space
cyan gate.

Grass, shrubs, reeds, leaves, and twigs are deterministic shared-mesh foliage
with no gameplay collider. Grass uses overlapping 3.2-metre shared macro patches
containing 9,216 individually oriented ribbons: each nearby blade samples a
cubic longitudinal curve with fifteen vertices, then cross-fades to a stable
1,600-blade subset using seven vertices per ribbon. The retained blades widen by
the square root of the density ratio, preserving aggregate coverage while
eliminating the rejected blades before vertex shading. Internal blade spacing
matches the former one-metre patch, so the larger footprint cuts grass render
entities by roughly an order of magnitude without reducing near-field density.
Canopy, water, cultivation, and snow select stable blades within the shared mesh
rather than opening macro-patch-sized holes. Both representations evaluate the
same authored lean, layered spatial wind, and player displacement curve, and an
edge-on ribbon turns partially toward the view so it retains useful screen width
without becoming a full billboard. Grass placement is not clipped to the
authoritative playable heightfield. One globally aligned placement domain spans
both playable terrain and the first presentation-only vista ring. Every eligible
location owns the same overlapping camera-distance-selected near, far, and vista
representations; the gameplay boundary only switches the source of height and
environmental coverage data. LOD fade distance is evaluated at each blade root
and complementary dithering is applied in the foliage fragment shader, so a
3.2-metre patch cannot switch as one visible square. A single continuous
coverage mask copies authoritative playable detail, then blends over twelve
metres into regional sward coverage; the playable rectangle itself is never
baked in as a grass-free border. Near ribbons fade into seven-vertex far
ribbons, which fade into 6.4-metre patch impostors containing 576 broad,
five-vertex tuft silhouettes. Regional coverage is sampled continuously per
blade rather than quantized into four patch-wide density levels or used to
discard entire macro patches, so ecological variation does not create square
holes. These are materially different geometry rather than blades merely
discarded in the vertex shader, following the high/low geometry and far-field
impostor division described in Eric Wohllaib's GDC 2021
[*Procedural Grass in Ghost of Tsushima*](https://gdcvault.com/play/1027033/)
talk. They fade by 140 metres. Beneath every geometric tier, the terrain keeps
the local solid soil, litter, mud, cultivation, or stone substrate instead of
painting grass green onto the ground; the blades alone provide the near-field
sward color. Only while the final vista blades fade from 124 to 140 metres does
the terrain introduce an optical-average molded-plastic pigment derived from the
same environmental grass palette. It compensates for the species/cohort
darkening, blade occlusion, and thin-foliage lighting that act after the blade
input color. This solid terminal LOD carries the field without sub-pixel
geometry or exposing a differently colored silhouette when the last blades
disappear. Regional vista vertices retain the same environmental samples,
including an aggregate sward-coverage channel, and hard world-space coverage
dithering selects that same calibrated pigment through every vista ring. Vista
slopes use continuous height-gradient normals instead of per-cell face normals;
sufficiently exposed hilly samples reuse the generated two-color rock surface
through coarse-safe triplanar sampling. The locally controlled player's position
and velocity flatten and push nearby grass as a presentation-only effect.

Grass is organized into deterministic, roughly 24-metre coherent plant
communities rather than selecting an unrelated species for every ribbon.
Mesic lowland swards combine tall false oat-grass (`Arrhenatherum elatius`)
with broader, clustered cocksfoot (`Dactylis glomerata`); lean or exposed
swards combine fine red fescue (`Festuca rubra` aggregate) with airy common
bent (`Agrostis capillaris`); damp openings combine tufted hair-grass
(`Deschampsia cespitosa`) with Yorkshire fog (`Holcus lanatus`). Existing
wetland, water, cultivation, hilliness, moisture, local vista samples, and
stable low-frequency site fields are temporary habitat inputs until world data
carries vegetation communities. Dry sites assign no token wet-tussock cells.
Species presets change physical blade height, width, pigment region, and
near-LOD panicle form. Articulated rachis and branch ribbons carry crossed
two-plane spikelets, distinguishing open oat, bent, and hair-grass panicles
from denser cocksfoot clusters. These rigid seed-head clusters inherit the
parent stalk's wind and player-interaction bend at their attachment point
without being reconstructed as blade ribbons. Far and vista LODs drop that
sub-pixel geometry while preserving the same patch footprint and community
identity.

Ordinary temperate understory shrubs use shared procedural common-hazel
(`Corylus avellana`), blackthorn (`Prunus spinosa`), and common-hawthorn
(`Crataegus monogyna`) presets rather than unique meshes per scatter point. The
reusable multi-stem shrub form parameterizes physical height, crown, stem-count,
shoot, leaf dimensions, petiole, bark relief, and leaf material. Stable
four-by-four scatter-cell communities create roughly 13-metre thickets: hazel is
weighted toward mesic shade and remains eligible on woodland leaf litter,
blackthorn toward bright dry scrub, and hawthorn toward open or cultivated edges
and woodland gaps. Community-scale density structure groups the same approximate
shrub population into dense cores, loose margins, and open relief instead of
distributing every specimen at a uniform cadence. Cambered near leaves and flat
alpha-card far leaves share generated, palette-constrained
albedo/opacity/normal/AO/roughness materials and the existing tree-leaf wind
shader. Mature tree presentation additionally supports common beech
(`Fagus sylvatica`). Its preset has a straight high-clear bole, compact
ascending scaffolds, smooth gray bark relief, and ovate subtly wavy leaves
instead of reusing oak roots, fissures, or gnarling. Stable 30-metre communities
weight beech toward moist, closed-canopy woodland. The same local community
selector does not delete grass macro patches: actual overlapping crown-litter
footprints and the scene-wide canopy density suppress the forest floor, avoiding
square holes in otherwise unoccupied habitat cells. The species-specific bark
and leaf palette is carried through playable and regional tree LODs, including
the software-baked whole-tree billboard. For now the compact server obstacle
remains the conservative generic `Tree` collider and the client derives oak
versus beech deterministically from replicated scene environment and position;
an explicit species field can replace that temporary selection rule without
changing generator recipes. Root self-shadow and a darker centre rib are
occlusion responses; a small solid-color palette and softened upward normals
keep the dense field readable without making individual cards look heavily lit.
A procedural terrain material selects hard-bounded forest floor, dry ground,
mud, cultivation, stone, water, and snow albedo regions. Both playable and vista
ground omit sampled albedo. Near walkable soil adds one shared 1024-square RG8
height/AO sample with a complete mip chain; world-XZ mapping and screen-space
height derivatives perturb the geometric normal without storing a normal map.
The two-metre tile represents sub-two-centimetre compacted earth, hollows,
clods, and fine aggregate, then fades from 12 to 24 metres and is suppressed on
steep, stone, gravel, water, and snow-covered surfaces. A camera-local 40-metre
detail patch refines the underlying geometric normal response to a 25-centimetre
grid without changing the authoritative two-metre collider. Its world-space,
centimetre-scale relief is deterministic and fades to the source surface over
the outer 4.5 metres. The residual height is signed and bounded to roughly seven
centimetres down and ten centimetres up: broad soil undulation and sparse clods
break up flat ground; local terrain gradients orient narrow drainage rills
downhill and soil-creep ridges across slopes; concavity gathers a shallow
sediment layer. Water stays flat. Road samples infer the local road axis and
edges from authoritative ground semantics, then form a raised crown and paired
compacted wheel ruts. Nearby replicated tree positions add a shallow mound,
basin, and several long curved radial root ridges, tying the ground silhouette
to the generated roots instead of applying unrelated noise. Stone and gravel
suppress soil clods in favor of broad contour-following bedrock ledges and
sparse intersecting fractures. Nearby replicated boulders carve a shallow visual
socket, build a contact apron, and deposit a widening downhill debris tail, so
the rock mass meets the landform instead of resting on a flat plane. These
obstacle-aware residuals remain presentation-only and do not alter the
authoritative terrain height or conservative colliders. These process layers are
geometry, never baked albedo or normal detail. A camera-centred shader cutout
removes the coarse base only beneath the safely covered patch interior, allowing
signed channels to remain visible instead of being depth-occluded. The outer
1.5-metre overlap retains the base terrain and a small fixed-function depth bias
resolves the almost fully morphed, coplanar perimeter without a second texture
or non-WebGPU shader stage. The patch snaps in one-metre increments and retains
the nearest vista height samples, so its quality radius follows the camera
across the playable boundary. Its PBR response modestly expands the lateral
components of the actual refined geometry normals and lowers dry roughness
within the patch; this preserves solid albedo while keeping centimetre-scale
facets legible under the bright environment-light floor. This is bounded
CPU-generated triangle refinement that runs on WebGPU's ordinary vertex/fragment
pipeline, not unavailable hardware tessellation or native-only mesh shaders.
Tree-canopy ground is presented as a deterministic ecological sequence rather
than a binary grass cutout. A high-resolution signed distance field derived from
authoritative leaf-litter cover selects discrete dark-loam, litter, shaded-soil,
and open-soil colors, and grades grass density and height across a 4.8-metre
exterior band. Sparse fallen leaves, twigs, and multi-segment shade-plant
rosettes extend into the outer 3.2 metres while the deep core stays mostly bare.
These remain solid molded-material regions and procedural meshes rather than
being baked into the soil texture. Tree sampling follows canopy coverage; rock
sampling uses an independent deterministic roll scaled by hilly coverage, so the
two features do not suppress one another. Weather affects ground wetness/snow
tint, bounded rain or snow particles, wind drift, sunlight transmission, and
distance fog. Clear weather retains a subtle kilometre-scale contrast haze
beyond tactical gameplay range. At the playable boundary, the first vista ring
reuses exact edge heights and eases the solid substrate pigment over several
regional samples while preserving its independent geometric-sward coverage.
Coarse vista samples preserve local peaks and render as seam-sharing rings of
independently culled 32-by-32-cell mesh chunks out to 50 km. Chunking changes
only CPU/ECS submission and culling granularity, not regional sampling or
visible tessellation, and avoids tens of thousands of startup-time entities;
coarser rings leave the playable and finer-ring interiors open rather than
overdrawing them. The 50-metre and 250-metre regional rings also
deterministically scatter bounded samples of the production whole-tree impostor
over canopy-bearing cells; the outer ring relies on aggregate canopy colour.
Those presentation-only trees share one cached atlas family, have no gameplay
collider, and are seated on the same morphed vista surface as the terrain. Tree
stand sampling scales with physical cell area instead of capping the first
250-metre ring to three silhouettes. Exposed terrain likewise continues rocks
beyond the playable rectangle: a shared procedural mesh hands off to a
twelve-face silhouette mesh, then to the vista terrain's aggregate rock palette
before either representation becomes subpixel. None of this vista scatter gains
collision or server authority.

Near-tree PBR leaf cards participate in the horizon-aware directional shadow
map, producing both cast shadows and leaf-on-leaf self shadow. Their vertex data
also carries a deterministic canopy-visibility term that darkens protected
interior foliage without relying on native screen-space ambient occlusion, which
is unavailable on WebGPU. The same alpha cutoff and CPU-synchronized wind phase
are used by forward, depth/prepass, and shadow passes so moving leaf silhouettes
remain aligned. Aggregate distant tree LODs do not cast individual leaf shadows.
Hazard mechanics remain future work;
[#212](https://github.com/adventure-simulator-group/fabelgeist/issues/212)
tracks that handoff and committed-result contract. The world map is a grid where
each square has a height and an enum for the terrain type. Each of these affect
both the speed of travel and the difficulty of climb/swim check to avoid injury.
We should not try and create our own, we should be able to find both height and
biome data from some open GIS dataset. At minimum, it should be easy to find
modern data for these, but there may also be a historical dataset that we can
use.

The strategic import records a bounded terrain profile on each road/ferry edge:
elevation, ascent/descent, grade, slope/aspect, roughness, relief, landforms,
water adjacency, and versioned seasonal/encounter tags. Because the historical
road source has endpoint topology but no polyline, these facts explicitly use
straight endpoint geometry. They select strategic travel and possible scenes;
a tactical server still owns all live terrain interaction.

## Height
Traveling from a lower height to a higher one may require the characters to make
a climb check (based on upper body strength vs weight) based on the slope, and
traveling from higher to lower may require the characters to make an agility
check.

## Terrain Type
As this is an enum, we can store extra information in each variant. For example,
the "River" variant could include its depth and velocity, both of which would
contribute to how hazardous it is to ford.

In addition to the costs imposed to traveling, it may also affect stealth and
detection. Forest cover may largely prevent detection from flying enemies but
also make it much easier for an enemy to ambush you, presenting a trade-off of
risk which you might assess based on which enemies are known to exist in an area
and which you are better able to defend against.

## Pathfinding
As described in the [travel](../strategic/travel.md) page.

# Weather
This probably isn't worth putting in the MVP, especially for such a temperate
place like Italy, but eventually there should be a weather map that affects the
way that you travel though terrain. Heavy snow would slow down land travel,
frozen lakes and rivers become possible to cross without swimming (but also
risky if the ice is thin), and rain makes climbing very difficult.

# Points of Interest
For the MVP, there is no need for any point of interest other than enemy
camps/lairs/nests relevant to active [quests](../strategic/quests.md) in an area
as well as [settlements](../strategic/settlement.md). The former are simply
placed randomly (though not too close to any other point of interest), the
latter should be obtained from a GIS dataset. If we can't find *historical* GIS
data on Italian settlements then we can just use modern data and rely on
[Cunningham's Law](https://meta.wikimedia.org../Cunningham%27s_Law) to fix it.

# Underground
In our setting, there is ostensibly a vast underground network of caves, crypts,
tunnels, Ratling under-cities, Dwarven strongholds, and even antediluvian ruins.
But this sounds hard, therefore we shouldn't bother with it for the MVP. All of
the quests will conveniently take you to overland locations, which don't even
need to have structures.
> Halbe: You are essentially being hired by local municipalities to clear out homeless encampments. If only we had this in the IRL modern setting...
## Foraging

Personal foraging trains only the Plains, Forest, and Hills leaf skills in the
normalized mixture of the character's current 1 km vicinity. The Terrain
heading remains a presentation aggregate and is never awarded or stored.
Cultivated ground affects legality, not the biome mixture. High Game, Low Game,
Fish, Harmful Beasts, and Plants divide one selected search-time budget; source
availability follows the local habitat rather than selecting another vicinity.
