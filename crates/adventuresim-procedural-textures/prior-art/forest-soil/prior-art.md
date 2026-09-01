# Forest soil prior art

## Scope

This report concerns exactly the `ForestSoil` recipe: the exposed mineral and
humic substrate beneath the separately authored `ForestLitter` layer. It covers
the procedural construction of soil aggregates, pores, clods, crumbs, shallow
hollows, and condition-dependent compaction/moisture; the relationship between
height, ambient occlusion, normals, and roughness; physical scale; tile and mip
behavior; packing; and acceptance tests.

Leaves, twigs, stones, roots visibly lying above the surface, and other discrete
forest-floor debris belong to other recipes or geometry. They are discussed
only where their contact with the soil establishes an interface requirement.

## Repository facts and constraints

The following are observations of the current repository, not claims from the
cited sources.

- `ForestSoil` is an implemented `Ground` recipe whose catalogue contract is
  only `PackedHeightAmbientOcclusion`. `ForestLitter` separately owns albedo,
  normals, height, and packed ambient/roughness/metallic output.
- The soil tile represents exactly 2 metres at 1024 by 1024 texels, or about
  1.95 millimetres per texel. Its declared physical height range is 0.028 m.
- The generator is exactly periodic and deterministic. A low-frequency domain
  warp breaks alignment while retaining that periodicity.
- The height hierarchy has broad undulation, hollows, cohesive clods,
  aggregates, detached crumbs, pores, and fine granular noise. Parent clusters
  fuse two to five irregular elliptical child lumps with probabilistic union.
- Broad compaction and moisture fields alter morphology rather than merely
  color. Moisture grows and softens cohesive masses; compaction lowers their
  count; dry loose patches support detached crumbs and grain.
- Pores are conditioned on saddle/contact bands between aggregates instead of
  being an unrelated population of dark spots.
- Packed mip zero is `Rg8Unorm`: R is normalized height and G is AO. Eleven
  mips are present. AO multiplies a half-resolution, four-direction horizon
  estimate at four radii by full-resolution local cavity at one- and four-texel
  radii.
- Existing tests cover bitwise determinism at sampled points, periodic edges,
  bounded condition fields, retained fine/mid/broad relief, wet-compaction
  suppression of loose relief, physical scale, AO cost and bounds, complete
  mips, non-growing height range, midscale survival through mip four, and an AO
  floor.
- There is currently no soil-owned base-color, normal, or roughness output.
  Recommendations for those channels therefore describe a future interface or
  a responsibility of the material that consumes `height_ao`; they should not
  silently broaden this recipe while independent texture agents are working.

## Evidence from practitioner workflows

### Build a material as a hierarchy of physical forms

**Evidence.** Material artist Gustav Engman describes a Substance Designer
forest floor as a height-first graph. Every height element retains a mask that
is reused for color and roughness, and the material is assembled in physical
order: soil first, then roots, stones, and other objects above it
([80 Level: Forest Ground Substance
Breakdown](https://80.lv/articles/forest-ground-substance-breakdown)). Jonjo
Hemmens similarly begins a forest-floor material by constructing primary and
secondary height forms, then adds dirt detail and scatters discrete assets with
masks that prevent implausible overlap
([Adobe Substance 3D: Foliage Art for Forest
Environments](https://www.adobe.com/products/substance3d/magazine/foliage-art-for-forest-environments-with-jonjo-hemmens.html)).

SideFX's terrain system embodies the same separation at a larger scale: a
height field can carry multiple named height and mask layers, with loose soil,
bedrock, debris, water, flow, and feature masks kept distinct until they are
composited
([SideFX: Building terrain with height
fields](https://www.sidefx.com/docs/houdini/model/heightfields)). HeightField
Erode treats loose material, deposition, rainfall, and flow as related fields,
and HeightField Slump distinguishes smooth, granular, and rainfall transport
with an explicit angle of repose
([SideFX: HeightField Erode](https://www.sidefx.com/docs/houdini/nodes/sop/heightfield_erode-.html),
[SideFX: HeightField Slump](https://www.sidefx.com/docs/houdini/nodes/cop/heightfield_slump.html)).

**Inference for this recipe.** The current semantic decomposition is a stronger
foundation than undifferentiated fractal noise. Keep named scalar fields for at
least broad elevation, compaction, moisture, hollows, cohesive clods,
aggregate union, contact/saddle band, crumbs, pores, and granular residue.
Compose the final height late. Exposing those fields to tests and preview modes
will make it possible to tune one physical phenomenon without accidentally
changing every output.

The hierarchy should have clear scale and amplitude ordering:

1. broad substrate undulation and compacted patches;
2. centimetre-scale hollows and cohesive clod masses;
3. millimetre-to-centimetre aggregate lobes and inter-aggregate saddles;
4. small crumbs attached near broken/contact zones; and
5. sub-texel or near-texel granular variation that affects shading more than
   silhouette.

The current graph already follows this ordering. Future work should sharpen its
semantic validation rather than add more anonymous noise octaves.

### Aggregates and pores are a coupled structure, not two scatter layers

**Evidence.** USDA-NRCS describes soil structure as micro-aggregates combined
into macro-aggregates by roots, fungal hyphae, microbial action, and organic
binders; the spaces within and between those aggregates form pores. It also
notes that compaction breaks larger units and collapses structure
([USDA-NRCS: Soil Tech Note 4A, Soil
Structure](https://www.nrcs.usda.gov/state-offices/illinois/soil-tech-note-4a-soil-structure)).
The agency's macropore guide distinguishes pores within aggregates from larger
pores between aggregates and notes that roots create and occupy macropores
([USDA-NRCS: Soil Structure &
Macropores](https://www.nrcs.usda.gov/sites/default/files/2022-10/Soil%20Structure%20and%20Macropores.pdf)).

The practical visual implication is also apparent in Substance workflows:
height masks are carried forward so later elements react to earlier ones rather
than being independently overlaid. In a SideFX forum example for procedural
straw/dirt debris, an artist constructs distorted repeated shapes, fakes their
contact shadows with blurred offsets, stacks the layers, and uses layered noise
for the remainder; the author explicitly calls out the cost of a 150-iteration
loop
([SideFX forum: procedural materials—straw, grass,
dirt?](https://www.sidefx.com/forum/topic/38768/?page=1)).

**Inference for this recipe.** Preserve the existing decision to derive pores
from aggregate saddles. Improve it by testing conditional association: most
strong pores should lie in the inter-aggregate/contact band, not at aggregate
peaks or in featureless broad soil. Detached crumbs should likewise favor
aggregate boundaries and dry/loose conditions. This covariance matters more
than the precise noise family.

Do not model every real microaggregate. At a 1.95 mm texel pitch, most physical
microaggregates and micropores are below representable scale. Their aggregate
effect belongs in roughness and the normal distribution, not as isolated R8
height spikes. The visible height map should represent clusters, clods, crumbs,
and larger voids.

### Moisture and compaction must change morphology and response together

**Evidence.** SideFX erosion and slump tools treat erodibility, debris depth,
deposition, rainfall, and repose angle as coupled controls. Granular slump adds
a bumpy distribution distinct from smooth transport, while rainfall produces
channels and local depressions; these are changes in shape and material
distribution, not tint alone
([SideFX: Slump](https://www.sidefx.com/docs/houdini/heightfields_cop/slump.html)).
USDA-NRCS identifies compaction as a collapse of larger structural units and
pore space, while roots and organic matter help bind aggregates.

**Inference for this recipe.** Retain the present condition-driven morphology.
For future shading, derive correlated material response from the same fields:

- dry loose soil: more detached crumbs and fine normal variance, high and
  spatially varied roughness, lighter exposed peaks;
- damp cohesive soil: larger softer clods, fewer detached grains, darker value,
  moderately lower roughness in compressed or water-rich recesses;
- compacted soil: smaller height variance, fewer macropore cavities, broader
  smooth transitions, and potentially a locally tighter highlight than loose
  crumb structure; and
- wet depressions: darkening and roughness reduction should follow low areas
  and moisture masks, not a globally multiplied noise.

These are art-direction inferences rather than a claim that an R8 condition
field is a soil simulation. The aim is coherent visual causality.

### Roots and organic matter need an ownership boundary

**Evidence.** Adobe's root-generation tutorial builds thick and thin root
networks separately, deforms them at multiple frequencies, blends them into
ground using a height blend, and derives normal, roughness, and AO after the
blend
([Adobe Substance 3D: Create
Roots](https://www.adobe.com/learn/substance-3d-designer/web/create-roots)).
Engman's bottom-to-top graph and Hemmens's masked scatter lanes likewise treat
roots and debris as elements laid over a soil base, not as coloration painted
inside it.

**Inference for this recipe.** `ForestSoil` may contain only subtle embedded
root impressions or organic binding variation at the substrate scale. Raised,
species-readable roots and litter must stay in `ForestLitter`, a dedicated root
recipe, or geometry. If a future compositor merges them, use height-aware
occlusion and contact masks so roots displace or compress the soil; do not add
root-shaped dark lines to soil albedo without corresponding relief.

## Height, normals, roughness, and AO

### Height should be the geometric source of truth, but not the source of every
### optical property

**Evidence.** Engman's workflow derives the normal map late from the assembled
height and reuses per-element masks to construct roughness. A practitioner PBR
tool likewise recommends tuning height first because normals and cavity AO
depend on it, but warns that roughness and AO inferred from a single image are
approximations rather than measured truth
([The Technical Artist: PBR Map
Generator](https://www.thetechnicalartist.com/tools/pbr-map-generator/index.html)).

**Inference for this recipe.** If/when soil normals are emitted, compute them
from the final physically scaled height, using the 2 m tile size and 0.028 m
range; do not tune normal strength independently in arbitrary texture units.
AO should remain a derived visibility/cavity term. Roughness, however, should
combine morphology masks with condition fields and independent bounded
microstructure. A direct `roughness = f(height)` mapping would make all peaks
and valleys optically identical and would incorrectly conflate elevation with
wetness or grain size.

The existing horizon-plus-local-cavity AO is directionally sound. Its present
four cardinal directions can, however, imprint axis bias on diagonal clods.
Before increasing sample count, measure rotational error by rotating a
synthetic mound/cavity through several angles. If visible, use an eight-direction
kernel or a rotated per-texel/sample pattern whose periodicity remains
deterministic.

### AO must remain restrained and must not become baked diffuse color

**Evidence.** Engman starts material review with very little AO to keep shapes
readable. The Technical Artist distinguishes height-derived cavity AO from
physically baked lighting. A SideFX MaterialX forum answer describes AO as a
mask for procedural dirt accumulation, with distance and cone angle controlling
its reach—not as a replacement for scene lighting
([SideFX forum: MaterialX Ambient Occlusion for procedural dirt](https://www.sidefx.com/forum/topic/82304/?page=1)).

**Inference for this recipe.** Keep the AO floor and review AO independently in
linear space. It should describe contact/cavity visibility at the tile's
physical scale and should not blacken broad soil patches. Do not multiply it
into a future base-color texture offline. If AO is used to modulate diffuse at
runtime, the material should retain explicit control so scene lighting does not
double-darken crevices.

## Tiling avoidance and semantic scale

### A seamless tile can still repeat visibly

**Evidence.** The procedural-noise literature distinguishes seamless borders
from the harder problem of avoiding distortion, blur, and obvious internal
landmarks when a tile repeats
([Game Developer, August 2011: Creating Seamlessly Tiling Perlin Noise for
Procedural Generation](https://media.gdcvault.com/GD_Mag_Archives/GDM_August_2011.pdf)).
Hemmens explicitly builds enough primary and secondary forms to avoid obvious
landmarks when the material tiles. Activision's large-scale terrain talk calls
out the collapse of tiled detail to a single color at distance and samples a
second, magnified/macro UV set, fading between regular and macro contribution
by camera distance
([Advances in Real-Time Rendering 2023: Large Scale Terrain
Rendering](https://advances.realtimerendering.com/s2023/Etienne%28ATVI%29-Large%20Scale%20Terrain%20Rendering%20with%20notes%20%28Advances%202023%29.pdf)).

**Inference for this recipe.** Exact periodicity is necessary but not a visual
acceptance criterion. The two-metre tile should be "quiet": avoid one dominant
hollow, clod, or diagonal warp that becomes a recognizable landmark in a 4 by
4 repeat. Test autocorrelation not just at the tile period but at strong
sub-periods introduced by the 3/5/7/10/15/34/53/68/97 frequency family.

Long-range variation should not be baked by making the tile physically huge;
that would sacrifice near-field texel density. Prefer a runtime macro field or
material/terrain-level condition field, sampled at tens of metres, that changes
moisture/compaction and modestly perturbs color/roughness. If the renderer can
afford it, stochastic rotated/offset tile sampling can hide orientation and
translation repeats, but the Activision talk notes that tile hiding can require
four material samples per shaded pixel. Measure that cost before adopting it.

### Frequencies need world-unit meanings

**Evidence.** Houdini's heightfield tools assume metres and expose feature size,
debris scale, and repose angle in spatial terms
([SideFX: HeightField geometry
node](https://www.sidefx.com/docs/houdini/nodes/sop/heightfield.html)). This makes
the graph portable across resolution changes and allows erosion/slump controls
to refer to actual terrain processes rather than texture pixels.

**Inference for this recipe.** Document the intended diameter band and height
amplitude for each feature class in metres. Keep texture resolution separate
from semantic scale. The current two-metre/1024 configuration implies roughly:

- broad fields: about 0.3–0.7 m wavelengths;
- hollows/cohesive masses: several centimetres to low tens of centimetres;
- visible aggregates and crumbs: a few millimetres to a few centimetres; and
- finer real particles: below the height-map resolution, represented only by
  roughness/normal statistics.

Those estimates should be verified against rendered rulers and reference
photographs. Tests should reject a resolution change that silently doubles the
physical size of clods or pores.

## Packing, mip behavior, and temporal stability

### Packing is an interface contract, not merely file organization

**Evidence.** Activision reports packing albedo/metalness and
normal/occlusion/gloss into two BC3 textures, or distributing those signals
across three BC1 textures to reduce memory by one third; the choice is driven by
compression quality, bit allocation, filtering, and available feature channels.
Terrain3D similarly documents engine-specific packs and insists that mipmaps be
enabled for terrain textures
([Terrain3D: Preparing
Textures](https://github.com/TokisanGames/Terrain3D/blob/main/doc/docs/texture_prep.md)).

**Inference for this recipe.** Preserve R=height, G=AO as a named linear-data
contract while it is what the consumer expects. Do not opportunistically place
roughness or color in a spare-looking channel without updating the catalogue,
runtime decode, import format, lab views, and tests together. Evaluate eventual
GPU compression on the packed data: height banding and AO cross-talk may make a
two-channel format preferable to a conventional color-oriented block pack.

### Ordinary averaging is safe for some signals and wrong for others

**Evidence.** Valve's GDC 2015 rendering talk shows that box-filtered normal
mips lose unresolved normal variance, producing incorrect glossiness at
distance. It states explicitly that averaging normals discards important
roughness information
([GDC 2015: Advanced VR Rendering
Performance](https://media.steampowered.com/apps/valve/2015/Alex_Vlachos_Advanced_VR_Rendering_GDC2015.pdf)).
Activision fades micro contribution out and macro contribution in with distance,
and notes that encoding camera-dependent content directly in virtual-texture
mips can introduce discontinuities between adjacent levels.

**Inference for this recipe.** Height can use low-pass mips for shading/parallax
lookup, but a mip height average is not a conservative displacement bound. If
the channel ever drives mesh displacement or collision, provide min/max bounds
or a separate representation. AO should be filtered as visibility in linear
space and remain bounded; do not average gamma-encoded values.

If future normals and roughness are generated, author semantic mips together:
renormalize the average normal and increase effective roughness according to
the discarded normal variance. Fine granular detail should fade before it
turns into moving sparkles. Macro compaction/moisture identity should survive
after individual crumbs and pores disappear.

## Concrete implementation recommendations

### Highest priority: preserve and expose semantic fields

1. Keep the current height hierarchy and promote its intermediate masks to a
   test/debug interface: broad, conditions, hollows, clods, aggregate, contact,
   crumbs, pores, and grain.
2. Add quantitative conditional tests: pore energy concentrated in saddle
   bands; crumb energy concentrated near aggregate boundaries; dry/loose crumb
   energy greater than damp/compact; compaction reducing cavity area and height
   variance; moisture increasing cohesive-clod footprint while softening edge
   gradients.
3. Add a physical-scale metadata table beside the recipe and assert feature
   statistics in metres rather than only noise frequencies or texel offsets.

### Next: improve AO and distance evidence without changing the output contract

1. Add rotation-isotropy tests for horizon AO using analytic synthetic fields.
2. Generate diagnostic images for height, AO, slope, curvature, each semantic
   mask, and a 4 by 4 tiled view. The tiled view should be inspected both near
   grazing and from a high, distant camera.
3. Measure two-dimensional autocorrelation and flag strong peaks at nontrivial
   sub-periods. Maintain a landmark score based on low-frequency connected
   components so one memorable clod cannot dominate the tile.
4. Test every mip for finite/bounded channels, monotonic loss of high-frequency
   energy, retained broad variance, seam equality, and no increase in AO
   darkness. Extend tests beyond mip six to the 1 by 1 level.

### Future material interface: correlated color, normal, and roughness

1. Do not duplicate `ForestLitter`. Decide explicitly whether soil shading is
   supplied by the terrain material or whether `ForestSoil` will graduate to a
   full surface recipe.
2. If it graduates, generate the tangent normal from physically scaled final
   height; build base color and roughness from semantic conditions and masks;
   keep metallic at zero; and retain AO as a separate linear signal.
3. Use dry/wet/compact condition masks for broad response, then add bounded
   micro-roughness below resolvable height scale. Derive semantic mips that fold
   lost normal variance into roughness.
4. Provide a compositor interface for roots/litter: height-aware placement,
   displaced substrate/contact AO, and shared condition masks. Raised roots and
   recognizable debris remain outside `ForestSoil`.

### Runtime anti-repetition, only after profiling

1. Add a low-frequency world-space condition field to vary moisture,
   compaction, base value, and roughness over areas much larger than two metres.
2. Fade fine soil normal/height contribution with projected texel size while
   preserving broad material identity.
3. Consider stochastic rotations/offsets or texture bombing only after capture
   evidence shows the quiet tile plus macro field is inadequate. Benchmark the
   additional texture samples and verify that transformed tangent normals and
   height derivatives remain correct.

## Acceptance plan

### Deterministic numeric tests

- Bitwise repeatability for all generated channels and semantic fields.
- Exact edge periodicity at mip zero and every generated mip.
- Physical height range, texel pitch, and per-feature diameter/gradient bands
  expressed in metres.
- Cross-field association metrics for pores, crumbs, aggregates, moisture, and
  compaction.
- AO flat-field identity, bounded cavity response, rotational error, and known
  sample budget.
- Per-mip height range, slope energy, AO histogram, seam error, and broad-field
  variance. For future normals: unit length and variance-aware roughness.
- Autocorrelation at tile and sub-period offsets; connected-component bounds
  for dominant low-frequency landmarks.

### Deterministic rendered captures

- Orthographic channel plates for height, AO, slope, curvature, and conditions.
- A PBR sphere/plane comparison under overcast, hard directional, and grazing
  light, with AO both enabled and disabled.
- One 2 m tile with a metric ruler and a human boot/footprint scale reference.
- A 4 by 4 and 8 by 8 repetition view from overhead and grazing angles.
- Near, middle, and far camera distances plus a slow lateral camera motion to
  expose granular shimmer, mip popping, or AO crawl.
- Dry/loose, damp/cohesive, wet/compact, and neutral condition wedges using the
  same seed, so morphological causality can be compared directly.

### Independent visual-review questions

- Does the surface read as structured forest soil rather than stones, stucco,
  oatmeal, or uniform fractal noise?
- Are aggregates visibly fused into irregular masses, with pores occurring
  between them rather than appearing as floating black dots?
- Do dry, damp, and compact conditions change form as well as value?
- Is the tile quiet enough that no clod or hollow becomes an obvious repeated
  landmark?
- Do small grains disappear stably with distance while broader compaction and
  moisture variation remain?
- Are cavities described without looking pre-baked, dirty, or double-shadowed?
- Are roots, leaves, and stones clearly owned by their intended layers rather
  than ambiguously painted into the soil?

## Pitfalls to avoid

- Adding more octaves of noise when the missing quality is semantic covariance.
- Treating pores as independent dark stamps or AO as albedo.
- Representing sub-millimetre particles as R8 height spikes at a 1.95 mm texel
  pitch.
- Letting one strong feature make the seamless two-metre tile recognizable.
- Using the same scalar to drive height, color, roughness, and AO without a
  physical reason.
- Baking litter or raised roots into `ForestSoil`, obscuring module ownership.
- Generating normal mips by averaging without compensating roughness.
- Assuming filtered height is safe for conservative displacement or collision.
- Expanding the packed channel contract without changing catalogue, consumer,
  import, lab, and tests as one reviewed interface.
- Spending four texture samples per pixel on stochastic tile hiding before a
  quiet tile and cheap macro field have been evaluated.

## Source assessment

The SideFX documentation and forum material establish common procedural-layer,
mask, erosion, slump, debris, and AO practices. The Substance/technical-artist
breakdowns provide direct evidence of height-first forest-ground authoring and
semantic mask reuse. The GDC/Advances talks establish production constraints
for multiscale terrain, packing, mip filtering, and temporal stability. USDA
sources are included only to ground the morphology of aggregates, pores, roots,
and compaction; the concrete real-time recommendations above are explicitly
marked as inference. Repository constraints were inspected locally and are
listed separately so they are not misattributed to external prior art.
