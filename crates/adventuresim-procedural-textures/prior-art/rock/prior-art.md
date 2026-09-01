# Rock procedural texture prior art

## Scope

This report concerns exactly the `Rock` procedural texture used on generated
boulders. It covers lithology-aware surface construction, the hierarchy from
geological structure to grains and pores, fracture and erosion direction,
physically meaningful albedo/normal/roughness/AO, triplanar application,
physical scale, tiling, mip behavior, and deterministic acceptance tests.

It does not redesign the boulder implicit field, the Surface Nets extractor,
or the server collision proxy. Those systems define macro silhouette and
gameplay volume. This texture should supply surface information too small to
justify geometry while remaining coherent with the selected lithology and
macro shape.

## Repository facts and constraints

The following are facts observed in this worktree, not external claims.

- `Rock` is an implemented `Stone` recipe with albedo, OpenGL normal, height,
  and ARM outputs.
- The generator currently emits a single 256 by 256 tile. Its declared physical
  tile size is 2 metres, or about 7.81 mm per texel, and its height range is
  0.04 m.
- The height is two summed periodic sine products. The albedo has only two gray
  values selected by height; roughness is one constant value; AO is a direct
  remap of height rather than a horizon or cavity estimate.
- Height-derived normals use the declared metre scale, which is a sound
  contract. The normal and height samplers are linearly filtered, while albedo
  and ARM are nearest-filtered. None of these four `Rock` images has an
  explicitly generated mip chain.
- The runtime supports three lithologies: granite, limestone, and sandstone.
  They all sample the same generic texture; a linear RGB multiplier plus a
  small roughness bias is the only lithology-specific material variation.
- Runtime boulders also have three macro archetypes: rounded, angular, and
  slab. An 18-cubed Surface Nets grid extracts their render meshes. The
  authoritative collision shape is a conservative sphere and must continue to
  contain the client mesh.
- The rock shader applies the shared tile in world-space triplanar projection
  at 0.5 tiles per metre. Since the tile is 2 m, this preserves the generator's
  stated physical scale. Each axis receives a seed-derived offset and a fixed
  rotation. Projection weights are the absolute macro normal raised to 2.5.
- The shader blends three albedo, ARM, and normal samples. It treats rock as a
  dielectric (`metallic = 0`), clamps perceptual roughness to 0.54–1.0, applies
  AO to diffuse and specular occlusion, and reduces normal-map strength by
  lithology.
- Tests currently prove deterministic macro meshes, containment by the server
  proxy, archetype distinction, bounded dielectric material settings, shader
  use of the three maps, a palette of at most two albedo values, at most two
  roughness values, and nontrivial normal/height/AO variation.
- The procedural-texture infrastructure is intended to let each recipe be
  iterated and visually accepted independently. A `Rock` improvement should
  therefore remain deterministic and reviewable in the texture lab as well as
  in a representative triplanar boulder scene.

These constraints imply that this iteration should improve the shared surface
contract first. Changing collision, inventing per-pixel runtime geology, or
requiring authored UVs would exceed the recipe's current responsibility.

## Practitioner evidence

### Begin with material identity, not a generic rock graph

The strongest practitioner workflows begin from reference and decide which
forms identify the material before building the graph. Senior material artist
Chris Hodgson describes collecting reference for form, color, complexity, and
composition, then deliberately choosing which details to reproduce because an
attempt to carry everything over makes a graph busy and unmanageable. His rock
face is built as separate visual components in a sequential height-first graph,
not as one undifferentiated noise stack
([Hodgson, 80 Level](https://80.lv/articles/chris-hodgson-building-material-in-substance-designer)).

The same principle appears in the SideFX `Rock Generator`: start from clean
geometry, fracture and reshape it, then add controlled variation while keeping
the workflow reusable and artist-directed
([Garilhe, SideFX](https://www.sidefx.com/tutorials/rock-generator/)). The
`Magic Market` series similarly separates high-resolution generation, proxy
and low-resolution outputs, displacement baking, curvature-driven texturing,
and parameter-wedged variants
([Sayed, SideFX](https://www.sidefx.com/tutorials/magic-market-procedural-rocks/)).

**Evidence-backed conclusion:** “rock” is too broad to be one surface identity.
The graph needs a small number of lithology-specific structural models whose
parameters are varied within bounded ranges.

**Inference for this repository:** the existing `RockLithology` enum is the
right high-level selector. Granite, limestone, and sandstone should not be
three tints over identical relief. They need distinct structure before color:

- granite: interlocking mineral domains, sparse larger crystals, joint-edge
  weathering, and granular disintegration;
- limestone: fine matrix, subtle bedding or depositional mottling, dissolution
  pits and softened solution channels;
- sandstone: directional bedding/cross-bedding, granular surface, differential
  cement erosion, and fractures constrained relative to beds.

This is also supported by geological observation rather than visual convention
alone. USGS's building-stone identification guide distinguishes sandstone by
rounded sand grains, limestone by fossil fragments or original beds, and
granite by mixed-color interlocking crystals
([USGS, *What Type of Rock Is It?*](https://pubs.usgs.gov/gip/acidrain/type.html)).
USGS describes sedimentary rocks as characteristically bedded and named in part
by grain size
([USGS, sedimentary-rock FAQ](https://www.usgs.gov/faqs/what-are-sedimentary-rocks?items_per_page=6&page=0)).

### A hierarchy of forms prevents noise soup

Hodgson's construction is explicitly hierarchical: shard-like primary rock
forms are scattered first; the same height is then used subtractively; water
erosion is directionally imposed; crafted branching cracks are added; sediment
bands conform to the large forms; and only then are fine porous details added
([Hodgson, 80 Level](https://80.lv/articles/chris-hodgson-building-material-in-substance-designer)).
Vincent Dérozier likewise describes a procedural cliff recipe in terms of
foundation height, procedural sculpting, major breakup, fine detail, then color
and roughness, with related variants developed as a coherent material
collection
([Dérozier, ArtStation](https://vincentderozier.artstation.com/projects/x3WWQW)).

SideFX's procedural rock-formation lesson names the same ordering directly:
base structure, secondary detail, then tertiary detail
([Perez, SideFX](https://www.sidefx.com/tutorials/procedural-rock-formations-1/)).
The SideFX forum discussion of VDB displacement also makes an important
technical distinction: displacing an SDF lookup with vector noise moves the
surface in space, while offsetting the field with scalar noise changes the
location of the zero surface. They are not interchangeable operations
([SideFX forum, VDB cliff displacement](https://www.sidefx.com/forum/topic/51115/?page=1)).

**Evidence-backed conclusion:** natural richness comes from different
operations at different semantic scales, with masks inherited from the large
forms. Summing unrelated noises at arbitrary frequencies is not an equivalent
workflow.

**Inference for this repository:** construct one normalized physical height
field in four named bands:

1. **Structural surface, 0.25–2 m.** Very low-amplitude bedding, mineral-domain
   masses, or solution basins. Most metre-scale silhouette must remain in the
   mesh, so this band should only imply structure rather than redraw it.
2. **Fracture/weathering forms, 30–250 mm.** Sparse joints, flakes, bed
   separations, pits, or exfoliation edges. This should dominate the texture's
   readable shape.
3. **Grain/aggregate forms, 2–20 mm.** Sand grains, calcitic pores, or granite
   mineral boundaries appropriate to the lithology and current texel scale.
4. **Micro response, below roughly 8 mm.** At 7.81 mm per source texel this is
   primarily roughness variance, not trustworthy explicit height.

Every band should have a separately testable amplitude and spatial-frequency
budget. Fine bands should be gated by structural masks instead of covering the
whole tile uniformly.

### Fracture and erosion have directions and causes

The National Park Service separates weathering, which breaks or chemically
alters rock in place, from erosion, which transports the result. Freeze-thaw
and root growth enlarge existing joints; dissolution removes soluble mineral
material
([NPS, Weathering](https://www.nps.gov/subjects/erosion/weathering.htm)). This
matters artistically: a crack network, a dissolution pit, and a downward water
streak should not be produced by the same isotropic field.

Hodgson uses anisotropic, asymmetric downward blur for water erosion, manually
constructs a branching crack, and warps sediment bands to conform to the rock
surface. The orientation is part of the phenomenon
([Hodgson, 80 Level](https://80.lv/articles/chris-hodgson-building-material-in-substance-designer)).
The production-oriented SideFX forum advice for procedural desert rocks begins
with Voronoi fracture and VDB fracture, but the artist specifically needs angle
constraints because unconstrained displaced pieces do not resemble sandstone
terracing
([SideFX forum, desert-rock fracture](https://www.sidefx.com/forum/post/258409/)).

Geological examples reinforce the lithology split. NPS documents sandstone
cross-bedding, vertical joints, calcite-cement dissolution, and exfoliating
slabs
([NPS, Arches rock strata](https://www.nps.gov/arch/learn/nature/rock-strata.htm));
USGS describes spheroidal granite weathering as preferential breakdown at
fracture intersections, progressively rounding blocks and producing coarse
quartz/feldspar-rich sediment
([USGS, granitic landforms](https://pubs.usgs.gov/of/2004/1007/granite.html)).

**Inference for this repository:** give each lithology one coherent 2D material
frame and transform it independently for each triplanar axis. In that frame:

- sandstone bedding has a dominant tangent with modest waviness; cracks are
  mostly bed-normal or follow bed boundaries; grain loss and streaking follow
  gravity only where a runtime gravity-aligned projection is actually known;
- limestone uses weaker bedding, connected dissolution paths, rounded pits,
  and sparse calcite/fossil-like inclusions rather than hard cellular cracks;
- granite uses two or three joint directions over interlocking mineral cells,
  with broader rounded weathering near joint intersections rather than sediment
  bands.

The current triplanar material has no object-space geological “up” in its
texture generator and rotates each projection plane differently. Therefore
strong gravity streaks or bedding that must remain continuous across projection
seams are a runtime orientation problem, not something a generic 2D tile can
truthfully solve. Keep such cues subtle until the shader carries an explicit
geological frame.

### Height should drive related channels, not contaminate them

Practitioners commonly solve the large and medium height structure before
normal, AO, color, and roughness. Tyler Oliver's sandstone workflow builds
height first, derives normal/AO/curvature, and then uses curvature plus separate
grunges and masks for color
([Oliver, 80 Level](https://80.lv/articles/making-a-beautiful-sandstone-temple-in-3ds-max-substance-3d-ue4)).
Tom Martyn likewise describes completing grayscale sculptural form before
normal, AO, roughness, and albedo
([Martyn, ArtStation](https://www.artstation.com/blogs/tommartyn/q98Y/desert-scene-wip-05-learning-substance-designer)).

That relationship is not “copy height into every output.” Vishal Ranga's
production rock shader packs distinct masks: edge/curvature, AO/dirt, and
scratches/cracks. His detail textures use deliberately directed slope blurs and
non-uniform directional warps
([Ranga, 80 Level](https://80.lv/articles/rock-shader-pipeline-from-zbrush-to-unreal)).
Hodgson uses component masks to blend roughness values and adds wet streaks as
a separate roughness phenomenon; curvature and AO affect color and roughness
only subtly
([Hodgson, 80 Level](https://80.lv/articles/chris-hodgson-building-material-in-substance-designer)).

Adobe's current OpenPBR guidance identifies stone as a dielectric: it retains a
diffuse response and mostly colorless specular reflection, while specular
roughness controls highlight width
([Adobe OpenPBR overview](https://experienceleague.adobe.com/en/docs/substance-3d/general-knowledge/openpbr/openpbr-overview)).
Adobe also states that normal, roughness, displacement, and AO are non-color
data and should not receive display color transforms, while base color does
([Adobe color-management guidance](https://experienceleague.adobe.com/en/docs/substance-3d/ecosystem/renderers/color-management/color-management)).

**Inference for this repository:** retain the correct `metallic = 0` contract,
but generate channels from shared semantic masks:

- **Height:** structural relief plus bounded fracture/pore subtraction, in
  metres. Do not let the 40 mm range imply that every high-frequency grain is
  centimetres tall.
- **Normal:** derive from the physical height gradient. Add micro-normal only
  if it represents structure above the texel/Nyquist limit and can be filtered
  correctly.
- **AO:** estimate local or multi-radius horizon occlusion from height. Do not
  use signed height as a proxy: an exposed low plane is not necessarily
  occluded, and a high point beside an overhang can be.
- **Albedo:** choose lithology palettes from unlit reference. Modulate with
  mineral/grain domains and subtle weathering masks, not illumination, AO, or a
  binary height threshold.
- **Roughness:** start high but nonconstant. Grain size, fresh fracture faces,
  clay/dust, dissolution polish, and moisture residue can change roughness even
  where height barely changes. Keep these variations broad enough to survive
  filtering.
- **Metalness:** remain zero. Decorative mica sparkle is not bulk metallic
  behavior and should not be represented by random metallic pixels.

### Physical scale is an authoring input

Adobe Designer exposes a graph physical-size contract precisely so preview
tiling can represent a material at its intended scale
([Adobe Designer material properties](https://experienceleague.adobe.com/en/docs/substance-3d-designer/using/workspace/3d-view/material-properties)).
SideFX's rock pipeline bakes high-resolution detail into displacement and
surface maps on the low-resolution asset, preserving a defined relationship
between modeled shape and texture detail
([Sayed, SideFX](https://www.sidefx.com/tutorials/magic-market-procedural-rocks/)).

**Inference for this repository:** 2 m at 256² is viable for fractures and
coarse grains, but it cannot represent convincing sub-millimetre crystalline
sparkle, sand asperities, or limestone microporosity as height. The generator
should express every feature width and amplitude in metres, then convert to UV
or texels. Tests should reject important features thinner than about two source
texels (15.6 mm) unless they are intentionally roughness-only and demonstrably
stable after mipping.

At the same time, the mesh's 18³ sampling is much coarser than the texture.
Large chips and planar breaks belong in `rock_field`; the 2 m texture should not
fake silhouette-scale ledges that never affect the outline or contact shadow.
Use a diagnostic overlay that labels each feature as **mesh**,
**height/normal**, or **roughness/albedo**, and reject duplicated scale bands.

### Triplanar mapping solves UV stretch, not geology

World-space triplanar projection is a sensible fit for procedurally generated
meshes without stable UVs. A GDC-era production presentation describes the
method succinctly as tiled UVs from vertex position for each axis, sampling
albedo and normal
([Doiron, GDC Online](https://media.gdcvault.com/gdconline11/jean-philippe_doiron_next-gen-game-in-flash-3d_developping-next-gen.pdf)).
Modern texture-pipeline work also treats triplanar mapping as a reusable sampler
module that can combine with other surface modules
([Palko, GDC 2025](https://media.gdcvault.com/gdc2025/Slides/Palko_Martin_Revolutionizing_Texture_Pipelines.pdf)).

However, three projections mean three texture evaluations per channel, and
normal maps need correct basis conversion before blending. The current shader
does both. Its fixed per-axis rotations and seed phase reduce obvious alignment,
but they also prevent a strong 2D sedimentary direction from continuing
geologically around a boulder.

**Inference for this repository:** keep triplanar projection for this bounded
iteration, with these rules:

- compute all projection UVs from the same metre-based scale;
- preserve the current per-axis tangent-basis reconstruction and test its sign
  conventions on all six cardinal normals;
- keep projection weights smooth enough to avoid seams but sharp enough that
  three contradictory directional patterns do not muddy together;
- use the recipe seed for phase and perhaps a small set of rotations, not for
  independent channel offsets; albedo, ARM, and normal must remain registered;
- avoid embedding a lighting direction or absolute “top” into the tile unless
  the shader exposes and applies an object/geological frame;
- measure the cost before adding texture bombing. The material already samples
  nine texels per fragment (three maps across three axes).

### Tiling needs structured variation, not destroyed landmarks

The SideFX production tutorial generates controlled wedges/variants rather
than expecting one texture to make every rock unique
([Sayed, SideFX](https://www.sidefx.com/tutorials/magic-market-procedural-rocks/)).
SideFX forum advice also recommends varying the position/offset that drives
noise per generated copy, which changes the realization without changing the
underlying procedural system
([SideFX forum, unique rocks](https://www.sidefx.com/forum/topic/104399/)).

**Inference for this repository:** use three layers of anti-repetition:

1. Make every field toroidally periodic so the base tile has exact C0 seams and
   its derived normals have matching gradients across the seam.
2. Derive a small deterministic atlas or seed-wedged set per lithology only if
   review proves one tile's landmarks repeat on ordinary boulders. Share the
   structural algorithm and parameter bounds; do not make unrelated noise
   soups.
3. Apply cheap runtime phase/rotation choices coherently to all channels. If a
   macro color modulation is needed, compute it from low-frequency world or
   object position outside the repeating tile rather than distorting its PBR
   channels independently.

The current 2 m tile at 0.5 tiles/m repeats once per 2 m, so a typical one-metre
boulder may not show a complete repetition on one face. The first priority is
therefore not stochastic bombing; it is removing the tile's conspicuous
sinusoidal signature and making the content lithologically meaningful.

### Mips must preserve material response

Valve's VR rendering presentation demonstrates that box-filtering normal maps
produces incorrect glossiness when viewed at distance
([Vlachos, GDC 2015](https://media.gdcvault.com/gdc2015/presentations/Vlachos_Alex_Advanced_VR_Rendering_V2.pdf)).
Ready at Dawn's GDC material recommends modifying roughness during filtering to
reduce specular aliasing, based on normal-map frequency
([Pettineo, GDC 2014](https://media.gdcvault.com/GDC2014/Presentations/Pettineo_Matt_Crafting_A_Next-Gen.pdf)).
Treyarch's physically based lighting work similarly calls out roughness mip
augmentation from normal-map variance
([Lazarov, Advances in Real-Time Rendering 2011](https://advances.realtimerendering.com/s2011/)).

**Inference for this repository:** explicit, semantic mip chains are required
for the revised rock:

- albedo mips average in linear-light space, then encode to sRGB;
- normals are decoded, averaged, and renormalized rather than byte-averaged;
- roughness increases as unresolved normal variance grows (Toksvig/variance
  compensation or a validated equivalent), preventing glitter and temporal
  shimmer;
- AO averages conservatively toward unoccluded, since microscopic dark cavities
  should not turn a distant boulder uniformly gray;
- metallic stays zero and channel registration remains exact;
- height mips retain the mean surface and reduce high-frequency amplitude. If
  height is not sampled at runtime, it still needs coherent review mips rather
  than misleading nearest-level previews.

The current absence of explicit mip chains and nearest filtering on albedo/ARM
is a concrete repository deficiency, not merely an aesthetic opportunity.

## Recommended bounded implementation

The following is a repository-specific proposal inferred from the evidence.

### 1. Make the recipe lithology-aware at generation time

Generate a separate `SurfaceTextureSet` for granite, limestone, and sandstone,
or a deterministic array/atlas with one region per lithology. Do not preserve a
generic shared surface plus tint as the final API: backward compatibility is
not a project requirement, and the current abstraction prevents the texture
from expressing the enum's meaning.

Keep shared helpers for periodic distance fields, physical-gradient normals,
horizon AO, mip construction, and channel validation. Share mechanisms, not
the actual relief realization.

### 2. Construct named masks before channels

For each lithology create deterministic fields such as:

- `structure`: mineral domains, bed bands, or depositional mottling;
- `fracture`: sparse connected joints with width, depth, and directional family;
- `weathering`: dilation/rounding or dissolution derived from fracture and
  structure exposure;
- `aggregate`: lithology-specific grain/crystal/pore field;
- `fresh_face`: recently exposed fracture surfaces;
- `deposition`: dust/clay/organic accumulation in genuinely sheltered lows.

Build height from these masks in physical units. Derive normals, curvature, and
multi-radius AO. Then synthesize albedo and roughness from the semantic masks
with small independent variation. This preserves cross-channel correlation
without copying one scalar into every output.

### 3. Use restrained lithology signatures

- **Granite:** 3–5 interlocking mineral colors, irregular domains in the
  10–60 mm band, limited crystal relief, sparse joint grooves, and softened
  domains near joints. Avoid confetti albedo and bright specular pixels.
- **Limestone:** narrow warm/cool gray-buff palette, fine matrix, subtle
  depositional bands, sparse rounded dissolution pores at several radii, and
  perhaps rare subdued inclusions. Avoid making every limestone coral/fossil
  patterned.
- **Sandstone:** 1–5 mm grains are mostly below or near the current texel scale,
  so represent most grains in albedo/roughness; reserve height for 15 mm+
  aggregates, bed boundaries, differential erosion, and sparse bed-relative
  fractures. Avoid a uniform horizontal stripe filter.

Reference images should be well lit and ungraded. Palette targets should be
recorded as linear/sRGB values with source and assumed moisture state; “granite
gray,” “limestone yellow,” and “sandstone orange” are not adequate material
definitions.

### 4. Preserve the mesh/material division

Do not move the boulder's major planar fracture, slab thickness, rounded mass,
ground contact, or side chip into the texture. Instead, expose the same
lithology/archetype pairing in captures so reviewers can detect contradictions:
a rounded granite boulder may have spheroidal weathering, while a slab
archetype with sandstone should show bed-compatible surface cues.

The texture does not need to match the exact Surface Nets vertices, but it must
not imply centimetre-deep ledges that the silhouette and shadows deny.

### 5. Add explicit mip and sampler policy

Use complete mip chains and linear filtering with anisotropy for albedo, normal,
and ARM. Produce all mips together from the semantic source fields so normal
variance can modify roughness. Retain repeat address modes. If GPU compression
is later introduced, test the actual compressed outputs, especially normal
seams and low-contrast roughness.

### 6. Keep runtime cost bounded

The initial improvement should not add shader samples. Replace the texture
content and mip policy while retaining the existing nine triplanar samples.
Only after profile evidence should the project consider stochastic texture
tiling, macro overlay maps, height blending, or parallax. A classic GDC survey
of parallax occlusion mapping emphasizes that it trades extra per-pixel work for
the illusion of geometric detail and requires continuous LOD handling
([Tatarchuk, GDC Vault](https://www.gdcvault.com/play/1013219/Practical-Parallax-Occlusion-Mapping-for)).
That is not automatically justified for metre-scale background boulders.

## Deterministic acceptance plan

### Generator invariants

- Same lithology and seed yields byte-identical base levels and mips.
- All three lithologies produce different deterministic signatures in height,
  albedo, and roughness—not only a runtime tint.
- Opposite tile edges match for height/albedo/ARM, and height gradients match
  closely enough that the derived normal has no seam.
- Every output has the expected full mip count and dimensions.
- Albedo is sRGB; normal, height, and ARM are linear data.
- Metallic is zero everywhere. Alpha is opaque everywhere.
- Normals decode to unit length within an explicit tolerance at every mip.
- Values are finite and within channel ranges; roughness remains inside the
  engine's intended dielectric range.

### Physical and structural tests

- Record tile size, texel size, relief range, and each feature band's intended
  metre interval as constants exercised by tests.
- Assert fracture widths, grain-domain widths, and bedding wavelengths have
  bounded distributions in physical units.
- Measure spectral energy in low, middle, and high bands. Reject a result where
  one generic octave stack dominates every lithology.
- Measure orientation histograms. Sandstone should have a bounded preferred
  bedding direction; granite mineral domains should not; limestone dissolution
  should be less rectilinear than granite joints.
- Compare channel correlations: AO should correlate with local/horizon cavity
  more strongly than with absolute height; albedo must not be a binary height
  threshold; roughness must have nonzero variation not identical to either.
- Downsample each source feature to its disappearance mip. High-frequency
  normal variance must increase or preserve effective roughness rather than
  producing a smoother, shinier distant surface.

### Triplanar shader tests

- Render six planar patches facing ±X, ±Y, and ±Z with the same world origin.
  Verify normal perturbations face outward and the projected physical scale is
  equal on every plane.
- Render planes rotating through blend zones, including near equal X/Y/Z
  weights. Compare adjacent frames or image differences for seams and
  brightness pumping.
- Verify albedo, normal, and ARM use identical phase, rotation, scale, and mip
  selection.
- Render the three lithologies on the same neutral sphere and on rounded,
  angular, and slab boulders. Review with fixed neutral daylight, fixed
  exposure,
  grazing light, and a roughness-debug view.
- Capture at close, middle, and far distances and during a slow camera dolly.
  Reject specular crawl, normal sparkle, moiré bedding, mip color shifts, or a
  sudden loss of the lithology signature.

### Tiling and art-direction review

- Produce a 3 by 3 flat tile sheet for every output and a 3 by 3 triplanar rock
  wall. Review seams separately from repeated landmarks.
- Place at least 25 same-lithology boulders with deterministic recipe seeds.
  The family should read as one geology without obvious cloned bright spots,
  identical cracks, or rotation pinwheels.
- Include grayscale height, normal, albedo-only, roughness, AO, and final-lit
  panels. A pleasant beauty render cannot substitute for channel correctness.
- Have an independent reviewer identify lithology from neutral captures with
  tint substantially reduced. If classification depends only on color, the
  structural goal has not been met.

## Common pitfalls to reject

- One fBm/Worley stack reused for height, albedo, AO, and roughness.
- Treating granite, limestone, and sandstone as palette swaps.
- Adding equal-amplitude detail at every octave (“noise soup”).
- Isotropic crack cells with no relation to bedding, joints, gravity, or
  weathering.
- Perfect Voronoi polygons, uniform-width cracks, or evenly spaced strata.
- Baking shadows, crevice black, or directional illumination into albedo.
- Computing AO from absolute height.
- Making every high point smooth/light and every low point rough/dark.
- Sub-texel height detail that aliases into sparkling normals.
- Bright mica-like metallic pixels in an otherwise dielectric rock.
- Strong horizontal bands in a 2D tile that rotate incoherently across
  triplanar axes.
- Independent seed offsets for albedo, normal, and ARM.
- Byte-averaged normal mips without renormalization or roughness compensation.
- Nearest-filtered albedo/roughness on a moving camera.
- Adding texture bombing or parallax before profiling the existing nine-sample
  shader and validating a simpler material.
- Letting texture relief duplicate or contradict the boulder's macro silhouette.
- Judging only a sphere preview; the actual Surface Nets archetypes and
  triplanar blend zones are required acceptance contexts.

## Source assessment

The cited SideFX tutorials and forums are primary practitioner evidence for
controlled procedural rock construction, scalar/vector field displacement,
fracture workflows, reusable variants, and the high-to-low texture boundary.
The 80 Level and ArtStation sources are first-person material and environment
artist breakdowns used for height hierarchy, semantic masks, directional
erosion, channel derivation, and production shader practice. The GDC and
Advances sources establish triplanar mapping, detail representation, and normal
mip/roughness stability. Adobe documentation supplies current PBR/color-space
contracts. USGS and NPS sources are used narrowly to prevent the artistic
recommendations from collapsing distinct lithologies into visual stereotypes.

Repository facts are isolated near the beginning. Every section labeled
“inference for this repository” is a proposed transfer, not a claim that a
source prescribed this exact Rust or Bevy implementation.
