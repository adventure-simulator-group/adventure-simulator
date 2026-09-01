# Forest litter prior art

## Scope

This report concerns exactly the `ForestLitter` procedural texture: the
continuous, tileable optical mass of leaves and decomposing organic debris on a
forest floor. It covers layered leaves, fragments, humus, small embedded twigs,
overlap height and ambient occlusion, directional distribution, physical scale,
PBR response, tile and mip behavior, and the boundary between texture relief
and runtime geometry scatter.

The underlying aggregate soil belongs to `ForestSoil`. Distinct raised leaves,
twigs, seedlings, stones, roots, and cones may use the same ecological masks,
but their close-range silhouettes belong to geometry once a flat height/normal
representation cannot reproduce them.

## Repository facts and constraints

The following are facts observed in the repository. They are separated from
external evidence and recommendations.

- `ForestLitter` is an implemented `Ground` recipe catalogued as a full surface
  output. The current generator is physically coupled to `ForestSoil` through
  one `GroundTextureSet`, but the catalogue exposes the two recipes separately.
- The litter tile represents 4 metres at 1024 by 1024 texels: about 3.91 mm per
  texel. The declared relief range is 0.016 m.
- Three explicit strata are generated. The dense, highly decomposed lower layer
  uses a 40-cell grid; the middle layer uses 28; the sparser, recognizable upper
  layer uses 21. Decomposition decreases from lower to upper.
- Six deterministic shape classes exist: intact oak, curled oak, half leaf,
  torn fragment, skeleton, and humified fragment. Lower strata select mostly
  humified and torn material; upper strata select mostly intact leaves.
- Each occupied cell has a jittered centre, uniformly random planar angle,
  radius, aspect, phase, side, pigment, and a random stacking order. The
  top-order shape wins within a stratum. Strata are then composited lower to
  upper with coverage-weighted height, tone, vein, edge, lift, and contact.
- A separate humified-debris field fills broad pockets. Probabilistic-union
  coverage preserves some exposed-soil gaps rather than forcing a completely
  opaque carpet.
- Height, AO, tone, and coverage are packed in an `Rgba8Unorm` surface image.
  A separate `Rg8Unorm` normal stores X/Z components derived from physically
  scaled final height. Both images carry eleven mips.
- The terrain shader treats coverage as a material blend weight, not cutout
  opacity. It generates a three-band dark/mid/pale leaf palette from tone,
  blends litter over shaded soil, applies packed AO to diffuse occlusion,
  derives modest extra roughness from litter coverage, and uses height for one
  bounded parallax offset. The terrain remains opaque.
- Mip levels 0–2 use the normal image mip chain. Level 3 is regenerated from a
  reduced semantic set: only middle/upper intact, half, and torn leaves. Level
  4 onward discards individual shapes and regenerates broad humus/merged-litter
  fields. Thus distant mips do not merely average hundreds of tiny leaves into
  colored soup.
- Existing tests cover periodicity, density, exposed-soil gaps, AO depth,
  physical tile and relief scale, stratum size/decomposition ordering, distinct
  shape masks, stratum-specific class populations, bounded channel contrast at
  mid/far mips, reduced adjacent contrast with distance, and persistent broad
  tone/coverage variation. An ignored test writes channel plates, interpreted
  color, a 2 by 2 tile, and mip previews.
- Runtime ground scatter already provides the close representation. It batches
  modeled dry-leaf patches, opaque twig geometry, and woodland plants into
  24 m cells. Their local visibility ranges are respectively 16 m, 12 m, and
  14 m, with AABB safety padding. The terrain litter detail fades over 20–32 m,
  so continuous material remains underneath the geometry during handoff.
- Geometry uses four dry-leaf variants and three twig variants, is
  deterministically scattered from scene ground cover, and is not a shadow
  caster. The texture is therefore not expected to provide every nearby
  silhouette, but it must remain plausible between and beyond the meshes.

## Evidence from practitioner workflows

### Physically layer recognisable debris over a continuous substrate

**Evidence.** Gustav Engman describes a Substance Designer forest floor as a
height-first graph assembled in real-world order: soil first, then roots,
stones, and subsequent debris. Each height element retains a mask reused by
color and roughness
([80 Level: Forest Ground Substance
Breakdown](https://80.lv/articles/forest-ground-substance-breakdown)). Jonjo
Hemmens similarly constructs a dirt height base, divides scattered assets into
separate graph lanes, and feeds masks from earlier scatters into later ones to
prevent awkward overlap
([Adobe Substance 3D: Foliage Art for Forest
Environments](https://www.adobe.com/products/substance3d/magazine/foliage-art-for-forest-environments-with-jonjo-hemmens.html)).

Emily Bisset's procedural leaf floor uses the same tile-generator parameters to
produce persistent leaf masks, then layers leaves over mud and water. The
breakdown calls incorrect color/alpha blending between overlapping leaves an
early failure and notes that wrong height and roughness made the surface read
incorrectly
([The Rookies: Creating a Procedural Autumn Leaf Covered Floor in Substance
Designer](https://www.therookies.co/blog/breakdowns/creating-a-procedural-autumn-leaf-covered-floor-in-substance-designer)).

Ole Groenbaek, principal environment artist at Playground Games, separates a
forest floor into independently toggleable leaves, ivy, moss, grass, stones,
sticks, debris, water, and a stump. Dirt clumps are modeled; clumps, sticks, and
leaves are scattered in Houdini; the rest is authored in Substance Designer and
combined in a separate graph
([Ole Groenbaek: Forest
Floor](https://olegroenbaek.artstation.com/projects/zA3l6q)).

**Inference for this recipe.** Keep `humus`, lower, middle, and upper debris as
semantic compositing layers. Each instance needs a stable identifier/order and
a persistent mask shared by height, tone, AO, and future roughness. Do not
scatter independent leaf-shaped noise into each channel. The current field
model is appropriate, but it should make overlap state inspectable: visible top
class, supporting stratum, contact band, total layer count, and exposed-soil
mask.

### Overlap is a depth-order and contact problem

**Evidence.** Technical artist Michael Ekker creates fallen-leaf geometry from
cards, distributes it, then runs a Houdini Vellum simulation to obtain natural
pile-up, crinkles, folds, and collision. The settled geometry is tiled and
baked; front/back IDs allow later material control, and a ground plane fills
holes
([Michael Ekker: Fallen Leaves](https://www.michaelekker.com/projects/3de8Bo)).
Bisset likewise reports that overlapping leaves required correct color and alpha
order and that an incorrect overlap blend remained visibly wrong.

**Inference for this recipe.** The current random winner per stratum avoids
commutative blending but does not prove a physically coherent pile. Validate
four separate contracts:

1. upper objects occlude lower color and veins where coverage is strong;
2. their height is never pulled below a visible supporting layer by fractional
   coverage interpolation;
3. contact AO occurs along supported/lifted edges and between layers, not across
   an entire leaf; and
4. exposed substrate remains continuous through true gaps rather than leaking
   through a foreground leaf because different channels chose different
   winners.

A cheap deterministic alternative to offline cloth simulation is to give every
item a base order and support height, then construct a top-surface z-buffer.
When a leaf overlaps support, derive local lift from support gradient and leaf
edge/curl; derive contact from the difference between top and support height.
This makes AO and height agree. A more expensive reference bake can use a small
offline Vellum/cloth pile, but it should inform statistics and acceptance rather
than becoming a nondeterministic runtime dependency.

### Decomposition should change form, not just pigment

**Evidence.** The USDA Forest Service distinguishes litter—recognizable leaves,
needles, twigs, bark flakes, cones, dead moss and small decayed wood—from duff,
the decomposed organic layer below in which source material is no longer
distinguishable. It also separates fine woody debris into physical diameter
classes and observes that litter/duff depth varies with stand development and
disturbance
([USDA Forest Service: Environmental, structural, and disturbance influences
over forest floor
components](https://research.fs.usda.gov/download/treesearch/56649.pdf)).

Practitioner graphs follow the same transition. Bisset warps and cuts leaf
variants to show tearing and decomposition. Hemmens builds enough primary and
secondary forms to avoid generic noise but uses later dirt detail to merge the
base. Groenbaek exposes debris categories independently instead of recoloring
one repeated stamp.

**Inference for this recipe.** Preserve the current stratum-specific shift from
intact/curled leaves at the top to fragments, skeletons, and humified shapes
below. Strengthen it by correlating several properties:

- upper fresh-fallen material: species-readable silhouette, clearer veins,
  broader leaf-scale height, less soil-stained tone, more edge lift;
- middle weathered material: tears, half leaves, curl, mixed pigments, stronger
  local contact and occlusion;
- lower decomposed material: merged dark organic masses, residual skeletons,
  low relief, little species readability; and
- duff/humus: continuous irregular coverage and microtexture rather than
  discrete miniature leaves.

The current far representation appropriately converges toward broad humus and
merged litter. It should not retain vein-shaped contrast once individual leaf
width projects below a pixel.

## Direction, distribution, and tiling

### Random orientation needs patch-scale causes

**Evidence.** SideFX's foliage/scatter tools separate point placement from
prototype assignment, allow attributes to vary scale and orientation, and use
feature masks to restrict where objects occur. The tree leaf generator exposes
yaw, pitch, twist, roll, gravity, tropism, scale variation, variants, and packed
instancing rather than relying on one random rotation
([SideFX: Foliage](https://www.sidefx.com/products/houdini/world-building/foliage/),
[SideFX Labs Tree Leaf
Generator](https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_leaf_generator.html)).
Mick West's production-oriented scattering article argues that natural-looking
scatter is governed by underlying rules rather than pure independent random
placement; even a simple exclusion zone can outperform unstructured randomness
([Game Developer: Random Scattering—Creating Realistic
Landscapes](https://www.gamedeveloper.com/business/random-scattering-creating-realistic-landscapes)).

GDC production systems likewise use environmental fields. The *Serious Sam 4*
terrain talk uses vegetation density with elevation, slope, and convexity to
control materials and forest/miscellaneous-plant distributions
([GDC 2019: Four Million Acres—Vegetation Density and
Materials](https://media.gdcvault.com/gdc2019/presentations/Ladavac_Alen_Four_Million_Acres.pdf)).
An overview of *Saints Row*'s procedural systems generates grass and clutter
positions from terrain height, an artist-controlled force map, thresholds, and
world-position noise
([GDC 2022: An Overview of Procedural Content Generation in *Saints
Row*](https://media.gdcvault.com/GDC%2B2022/Speaker%2BSlides/An%2BOverview%2Bof_Todisco_Kevin.pdf)).

**Inference for this recipe.** Per-leaf angle should remain varied, but add a
low-frequency orientation field representing wind, slope flow, trunks, roots,
path edges, or local accumulation. Sample most angles from a broad distribution
around that field and retain a minority of unconstrained pieces. This yields
weak coherent streaks and pockets without making every leaf parallel.

Orientation must not be a globally preferred texture axis. Test circular angle
histograms both globally and within patches. Global distribution should be
approximately isotropic for a generic tile, while local neighborhoods may show
bounded anisotropy. If scene slope/wind is available only at runtime, keep the
base tile quiet and express directional piles through geometry scatter or a
world-space macro mask rather than baking one diagonal into every repeat.

### Seamless edges do not eliminate internal repetition

**Evidence.** Hemmens explicitly builds primary and secondary forms to avoid
obvious landmarks when a forest material tiles. Bisset observes that too-uniform
leaves hurt the composition and introduces warped/cut variants. Pete McNally's
scanned leaf-ground workflow initially produced obvious repeats; he masks bad
areas and synthesizes a seamless material, retaining diffuse, normal, height,
AO/cavity, and a fresher-leaf scattering mask
([Pete McNally: Seamless Autumn Leaves
Material](https://petemcnally.com/2018/03/24/seamless-autumn-leaves-material/)).

**Inference for this recipe.** Exact periodicity is only the first gate. A 4 m
tile repeated across a forest is vulnerable to memorable intact leaves,
high-contrast gaps, clusters, and diagonal alignments. Add:

- 4 by 4 and 8 by 8 tile captures at overhead and grazing views;
- two-dimensional autocorrelation for tone, coverage, and height;
- connected-component tests limiting the size and uniqueness of the strongest
  exposed-soil holes and pale upper leaves; and
- sub-period checks against the 21/28/40 placement grids and low-frequency
  humus/warp fields.

Use a large, low-contrast world-space litter-density/decomposition field to
break the 4 m repeat across many tiles. Stochastic rotated/offset texture
sampling is a later option, but it must transform normal derivatives correctly
and be profiled before adding multiple samples per pixel.

## Texel density and world scale

### Texture-scale leaves must be resolvable at the intended camera distance

**Evidence.** Adobe's PBR guide identifies document resolution and texel
density as direct causes of visible edge artifacts
([Adobe Substance 3D: The PBR Guide, Part
2](https://www.adobe.com/learn/substance-3d-designer/web/the-pbr-guide-part-2)).
Practitioner leaf-floor workflows bake settled geometry to height/normal maps
and inspect results under multiple lights rather than judging the source graph
alone.

**Inference for this recipe.** At 3.91 mm/texel, a 7 cm fragment spans about
18 texels, a 16 cm leaf about 41, and a 22 cm leaf about 56. This is enough for
broad silhouette and midrib at mip zero, but not for faithful tertiary veins,
fine serrations, thin skeleton ribs, or sub-millimetre edge curl. Those details
must either be deliberately widened, relegated to close geometry, or allowed to
disappear.

The current stratum tests imply roughly 7–13 cm lower fragments, up to about
21 cm middle forms, and about 16–23 cm upper forms. Treat these as an art
contract and compare them with a metric ruler and the species mix intended for
the setting. Do not resize leaf features by changing texture resolution. Keep
the 4 m physical tile constant and regenerate at a different resolution if more
sampling is required.

The 16 mm declared relief is a pile-scale bound, not individual leaf thickness.
Actual leaf material thickness belongs to geometry/card shading; height relief
represents pile elevation, fold, and support. A grazing capture should reveal
several millimetres of aggregate lift without making every printed leaf look
like a thick wooden plaque.

## PBR channels and material response

### Preserve intrinsic material properties separately from lighting

**Evidence.** Adobe's PBR guide defines AO as ambient accessibility, says it
should affect diffuse ambient contribution rather than specular, and explicitly
advises against baking AO into other texture maps. Roughness represents
microscopic surface variation, while non-metals such as organic debris remain
dielectric
([Adobe Substance 3D: The PBR Guide, Part
2](https://www.adobe.com/learn/substance-3d-designer/web/the-pbr-guide-part-2),
[Adobe Substance 3D:
OpenPBR](https://experienceleague.adobe.com/en/docs/substance-3d/general-knowledge/openpbr/openpbr-overview)).

Guerrilla's *Horizon Zero Dawn* vegetation shaders output normal, albedo,
roughness, reflectance, translucency amount/diffusion, depth, and motion
vectors, demonstrating that vegetation response is not reducible to one tinted
diffuse map
([GDC 2018: Between Tech and Art—The Vegetation of *Horizon Zero Dawn*](https://media.gdcvault.com/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf)).

**Inference for this recipe.** The current packed `height/AO/tone/coverage`
contract is efficient and coherent with its shader, but catalogue language
should not imply that tone is a complete albedo or that coverage is a complete
ARM map. Make the actual runtime channel semantics explicit in private module
documentation and the texture lab.

If the recipe later emits a fuller material response:

- base color: muted dry/decomposed pigments, no baked shadows;
- normal: derived from physically scaled top height, with sub-resolvable normal
  variance handled through roughness;
- roughness: high overall, but correlated with decomposition, dampness, exposed
  waxy leaf faces, torn edges, and soil staining—not simply height or tone;
- metallic: exactly zero;
- AO: restrained layer/contact visibility, separate from base color;
- coverage: terrain blend weight, explicitly not opacity; and
- optional transmission: only for close, thin, less-decomposed leaf geometry,
  not the merged far terrain mass.

The runtime shader currently raises roughness slightly for litter coverage and
sets geometry leaves to a very rough response. That is a reasonable baseline.
The missing evidence is whether upper leaf faces and damp compacted litter need
bounded roughness variation to prevent the whole layer reading as uniform felt.

### Height, normal, parallax, and AO must share one visible top surface

**Evidence.** Ekker bakes settled geometry and retains front/back IDs, which
keeps surface identity aligned across maps. Engman builds height first and
derives normals late. Bisset identifies tessellation, overlap blend, and
roughness errors together because they jointly control whether the pile reads
as leaves.

**Inference for this recipe.** Continue deriving normals from the final
physically scaled height and sampling surface/normal at the same parallax UV.
Add tests that the visible top-class mask agrees with tone and height at strong
coverage. AO should follow actual support/contact: a lifted edge may cast a
narrow cavity on support, but its exposed upper surface should not be darkened
uniformly. Review with AO disabled to ensure relief and palette still carry the
material.

## Mips, distance behavior, and temporal stability

### Semantic simplification is preferable to averaging tiny leaves forever

**Evidence.** Valve's GDC rendering work shows that box-filtered normal mips
lose unresolved normal variance and therefore imply the wrong glossiness at
distance
([GDC 2015: Advanced VR Rendering
Performance](https://media.steampowered.com/apps/valve/2015/Alex_Vlachos_Advanced_VR_Rendering_GDC2015.pdf)).
The broader terrain literature uses distance-dependent macro/detail
contributions because a small tiled material cannot preserve both near detail
and far identity with one unmodified frequency band
([Advances in Real-Time Rendering 2023: Large Scale Terrain
Rendering](https://advances.realtimerendering.com/s2023/Etienne%28ATVI%29-Large%20Scale%20Terrain%20Rendering%20with%20notes%20%28Advances%202023%29.pdf)).

**Inference for this recipe.** The current semantic mip regeneration is strong
prior-art-aligned design. Retain the three regimes:

1. full forms while leaf silhouettes span enough pixels;
2. a mid regime with only robust upper/middle forms; and
3. a far regime of aggregate humus/coverage with no individual leaves.

Improve the transition tests. Measure mean and variance of coverage, tone,
height, AO, and normal slope on both sides of levels 2→3 and 3→4. A semantic
change can be visually correct yet still pop if integrated coverage or mean
energy jumps. Crossfade or choose transition levels from projected physical
leaf width rather than fixed mip indices if runtime sampling makes the change
visible.

Normal mips should be regenerated from each semantic height field, as they are
now, rather than downsampling a normal belonging to discarded leaves. If a
roughness channel is added, fold unresolved normal variance into effective
roughness. Fine veins, skeleton ribs, and thin edge lifts should vanish before
they shimmer during camera motion.

### Geometry-to-texture handoff must conserve the forest-floor read

**Evidence.** SideFX supports packed instancing and prototype variants for
large foliage populations, while its scatter tools use masks and point
attributes to control placement. Jack McKelvie's work on Epic's *Electric
Dreams* GDC demo distinguishes a landscape auto-blend material from PCG
micro-scatter, explicitly testing pebbles and leaves as separate scatter layers
([Jack McKelvie: *Electric Dreams* PCG
environment](https://jackm.artstation.com/projects/DvmwAO)). Chris Copeland uses
simple leaf-mesh piles as ground scatter specifically to break up a landscape
and add real height variation
([Chris Copeland: Forest Scene UE4, Part
2](https://chriscopeland3d.artstation.com/blog/3m3V/gart-250-forest-scene-ue4-part-2)).

**Inference for this recipe.** Keep the continuous texture present under close
geometry. The meshes should add silhouettes, self-occlusion, and high parallax,
not replace the base and expose holes when culled. Coordinate the current
12–16 m mesh cutoffs with the texture's full→mid semantic regime and 20–32 m
relief fade:

- 0–12 m: texture mass plus leaves, twigs, and plants;
- 12–16 m: texture mass plus diminishing leaf geometry;
- approximately 16–20 m: texture full/mid representation carries the floor;
- 20–32 m: fade texture relief/normal while retaining stable aggregate color
  and coverage; and
- beyond 32 m: macro canopy-floor identity, without leaf-scale relief.

This is a conceptual acceptance sequence, not a mandate for these exact cutoff
values. Capture the actual renderer because AABB-padded batch visibility and
mip selection can shift perceived transitions.

## Concrete recommendations

### Highest priority: validate layering and physical scale

1. Expose diagnostic fields for visible class/stratum, top order, support
   height, layer count, contact, vein, edge, humus, and exposed soil.
2. Replace or constrain coverage-weighted height wherever it can pull a visible
   upper leaf below its support. Test top-height monotonicity under overlap.
3. Add cross-channel identity tests: strong-coverage top class must own tone,
   height relief, vein/edge, and local normal consistently.
4. Keep current physical tile and height constants; document each stratum's
   diameter distribution in metres and verify it with metric capture props.

### Add distribution causes without baking one obvious direction

1. Generate a low-frequency orientation/accumulation field independent of the
   placement grids.
2. Mix broad locally aligned angle distributions with a minority of uniformly
   random items. Vary alignment strength by stratum; decomposed lower material
   should be less direction-readable than upper leaves and twigs.
3. Accept optional runtime slope/wind/obstacle masks for macro placement while
   keeping the tile globally isotropic.
4. Test global circular uniformity, bounded local anisotropy, cluster size, and
   independence from U/V axes.

### Strengthen mip and handoff tests

1. Test every mip through 1 by 1 for bounds, seam equality, integrated
   coverage/tone/AO, and monotonic loss of high-frequency slope.
2. Compare both sides of semantic mip transitions and limit jumps in mean,
   variance, and dominant palette population.
3. Add slow lateral/orbit camera captures over the 12–32 m range with geometry
   counts and selected mip recorded. Reject silhouette popping, AO crawl,
   sparkle, and a visible brown ring.
4. Measure combined optical coverage with geometry on/off. The texture must
   remain plausible alone; geometry must enrich rather than double the floor
   into an implausibly dense wall of leaves.

### Clarify the surface contract

1. Document that packed surface channels are height, AO, tone selector, and
   terrain coverage; document that RG normal derives from the same height.
2. Keep all channels linear except any future actual base-color texture.
3. If explicit albedo/roughness/metallic is required by the generic catalogue,
   add it as a reviewed schema/runtime change, not by relabeling existing
   channels. Metallic remains zero.
4. Keep AO out of base color and limit it to contact/accessibility. Review with
   AO disabled and under several light directions.

## Acceptance plan

### Deterministic numeric tests

- Bitwise determinism and exact periodicity for every semantic field and mip.
- Per-stratum class populations, physical diameter bands, decomposition order,
  aspect distribution, and coverage.
- Global orientation histogram and neighborhood orientation correlation.
- Top-height/support monotonicity and winner consistency across height, tone,
  vein, edge, normal, and contact.
- AO association with true overlap/contact, plus AO floor and mean bounds.
- Soil-gap area, connectedness, and absence of one dominant unique hole.
- Autocorrelation at tile and sub-period offsets for tone, coverage, and height.
- Per-mip integrated coverage, tone, AO, height/slope energy, and semantic
  transition deltas.
- Normal unit reconstruction and agreement with physically scaled height.

### Deterministic visual captures

- Individual shape-class plates, then lower/middle/upper stratum plates.
- Height, AO, tone, coverage, normal, top-class, support, and contact channels.
- One 4 m tile beside a ruler, boot, intact oak leaf, and twig scale reference.
- 4 by 4 and 8 by 8 repeats overhead and grazing.
- Geometry off/on comparisons at 2, 8, 12, 16, 20, 24, and 32 m.
- Dry and damp variants under overcast, hard directional, and grazing light,
  with AO independently toggled.
- A slow camera translation and orbit to reveal mip/LOD popping, parallax swim,
  normal sparkle, and repeated landmarks.
- A decomposition wedge using one seed so upper leaf recognition can be seen
  merging into lower humus rather than merely changing color.

### Independent visual-review questions

- Does the surface read as overlapping deciduous litter rather than a printed
  leaf collage, gravel, scales, or brown noise?
- Are upper leaves recognisable while lower layers merge plausibly into duff?
- Do overlaps have a clear top/support relationship and narrow contact shadows?
- Are some edges lifted without making the whole tile look inflated?
- Is directional variation locally coherent but globally free of a repeated
  diagonal?
- Do exposed soil gaps look intentional and continuous at all distances?
- Does geometry add silhouette and pile height without causing a density pop
  when it appears or disappears?
- Do individual leaves simplify into stable aggregate litter before their
  veins, gaps, and normals shimmer?
- Does the material remain plausible with AO disabled and under moving light?

## Pitfalls to avoid

- Independent noise or scatter for height, color, coverage, AO, and roughness.
- Alpha-blending multiple leaves without a stable top-order/depth model.
- Treating terrain coverage as cutout opacity or pre-multiplying it into all
  channels without documenting that decision.
- Uniform random angles at every scale, or one global wind direction repeated
  by every tile.
- Making every lower-layer fragment a miniature species-readable leaf.
- Encoding sub-texel veins and skeleton ribs as high-amplitude height spikes.
- Baking AO into leaf pigment or letting contact AO cover an entire leaf face.
- Using leaf normal averages at distance without compensating lost variance.
- Cutting off geometry before the continuous material can carry its optical
  coverage, or making both representations so dense that the handoff doubles.
- Adding expensive stochastic sampling before a quiet tile, semantic mips, and
  a world-space macro field have been evaluated.
- Silently broadening or relabeling the packed output contract while other
  texture recipes and runtime consumers are being iterated independently.

## Source assessment

The cited Substance and Houdini breakdowns are direct practitioner evidence for
height-first layering, stable masks, overlap handling, simulated piles,
directional/prototype variation, and the texture-versus-geometry split. The GDC
materials establish production practices for environmental scatter fields,
vegetation PBR channels, distance representations, and normal mip stability.
The USDA Forest Service source is used only to ground the distinction between
recognisable litter, fine woody debris, and decomposed duff. Concrete proposals
for this repository are marked as inference, and current implementation facts
are listed separately above.
