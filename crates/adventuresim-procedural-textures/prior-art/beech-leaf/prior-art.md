# European beech leaf prior art

## Scope and species contract

This report concerns only the `BeechLeaf` recipe, interpreted as European or
common beech, *Fagus sylvatica*. That is the native beech appropriate to a
German environment. The target is a deterministic, species-readable, two-sided
leaf-card texture. Whole-tree architecture, branch placement, wind, and canopy
LOD are outside the recipe, except where they establish inputs that its texture
and card must support.

Beech is visually quieter than oak, hazel, or hawthorn. Its identity should not
be manufactured from extra noise. At leaf scale the useful cues are an ovate to
elliptic blade, acute tip, short petiole, gently undulating or shallowly blunt-
toothed margin, and a regular pinnate rhythm of strong secondaries. At canopy
scale its glossy upper surface, paler reverse, thin transmitted-light response,
and broad folded planes become more important than tiny edge detail.

## Repository facts and constraints

The following are observations of the current repository, not claims from the
cited sources.

- `BeechLeaf` is an `Implemented` foliage recipe with the stable slug
  `beech-leaf` and the standard leaf output contract: opacity, front/back
  albedo, front/back normals, height, and packed AO/roughness/metallic.
- It does not yet have a dedicated `beech_leaf.rs`; it passes through the
  shared generic `LeafRecipe` sampler in `foliage.rs`.
- The current parameters use a symmetric generic envelope, eight periodic
  margin oscillations, eight secondary-vein pairs, a slight axis bend, and one
  common width scale. The margin oscillation is independent of the vein graph.
- The generic secondary veins share one origin/reach formula. They angle from
  the midrib but do not explicitly terminate at corresponding marginal waves
  or teeth.
- Relief is a transverse dome plus a fixed-height vein contribution. The
  generic path has no interveinal folding, order-dependent vein taper, cuticle
  response, or species-specific front/back relief.
- The recipe uses front blade `[67, 108, 46]`, vein `[126, 147, 72]`, reverse
  blade `[81, 117, 59]`, roughness `214/255`, and zero metallic.
- The generic beech path emits 256 by 256 images without a generated mip chain.
  Other dedicated leaf recipes in this crate have nine semantic mips, so the
  absence is both a quality gap and a consistency gap.
- The recipe is renderer-independent. Runtime card geometry, two-sided
  material behavior, transmission, alpha cutoff, placement, and LOD remain
  separate owners.

## Botanical evidence and procedural implications

### The blade should be ovate to elliptic, acute, and only gently toothed

**Evidence.** The Biological Flora account describes ordinary *F. sylvatica*
leaves as alternate, ovate to elliptic, 4–10 cm long, with five to nine pairs of
veins and a 5–15 mm petiole. Young margins are markedly hairy and wavy; mature
leaves lose most hair while retaining variable scalloping or slight toothing
([Packham et al., Biological Flora of the British Isles: *Fagus
sylvatica*](https://besjournals.onlinelibrary.wiley.com/doi/10.1111/j.1365-2745.2012.02017.x)).
North Carolina State Extension independently describes an ovate to elliptic,
glossy dark-green blade, paler underside, entire undulate margin, and seven to
nine pairs of pinnate veins
([NCSU Extension: European beech](https://plants.ces.ncsu.edu/plants/fagus-sylvatica/common-name/european-beech/)).
Oregon State gives 5–10 cm length, a usually entire undulate margin, five to
nine vein pairs, an acute apex, and a short 0.5–1 cm petiole
([Oregon State University: *Fagus
sylvatica*](https://landscapeplants.oregonstate.edu/plants/fagus-sylvatica)).

A taxonomic treatment reports that the margin ranges from entire or crenate to
blunt-toothed, with the basal margin commonly entire/wavy and the apical margin
more likely to become inconspicuously or conspicuously dentate. It gives four to
eleven secondary veins, with seven or eight most frequent, and records ovate,
elliptic, and obovate forms
([Denk 1999: taxonomy of western Eurasian *Fagus*, part
2](https://www.researchgate.net/publication/247938731_The_taxonomy_of_Fagus_in_western_Eurasia_2_Fagus_sylvatica_subsp_sylvatica)).

**Inference for this recipe.** Eight secondaries are a sound default. Eight
equal sinusoidal teeth are not. Build a smooth ovate/elliptic envelope first,
then add low-amplitude waves or occasional blunt crenations whose phase is tied
to the secondary-vein termini. Keep the basal third calmer than the upper
margin. The two sides may vary slightly in width, vein origin, and crenation,
but should not become conspicuously jagged or lobed. A sharp repeated sawtooth
would read more like hornbeam or another serrate broadleaf than European beech.

### The secondary-vein rhythm is the main species-scale surface structure

**Evidence.** Oxford University's botanical profile calls out prominent
pinnate veins on the lower surface
([Oxford Plants 400: *Fagus
sylvatica*](https://herbaria.plants.ox.ac.uk/bol/plants400/Profiles/EF/Fagus)).
The taxonomic treatment places the modal secondary count at seven to eight and
describes venation ranging from brochidodromous through semi-craspedodromous to
craspedodromous according to leaf and margin form. In practical visual terms,
the secondaries are regularly spaced, travel strongly toward the edge, and may
either loop near the margin or terminate at a weak tooth.

**Inference for this recipe.** Treat venation as an authored graph rather than
a texture-frequency effect:

1. a tapered, slightly sinuous midrib extending through the petiole;
2. seven or eight secondary pairs with regular but non-identical spacing;
3. secondaries that gently curve and either meet a shallow crenation or turn
   toward a neighboring vein near an entire margin;
4. restrained opposite/percurrent tertiary links between adjacent secondaries;
   and
5. very faint finer reticulation only where it survives the intended close
   card scale.

The same graph should drive silhouette events, albedo, height, normals, AO, and
future transmission thickness. The current independent tooth and vein formulas
can visibly disagree, which weakens the otherwise distinctive beech rhythm.

### Young hair and mature gloss are states, not simultaneous generic noise

**Evidence.** NCSU and Oregon State both describe a lustrous dark-green upper
surface, lighter underside, silky/ciliate young margins, and a mature leaf that
becomes largely glabrous. The taxonomic account retains hair especially on the
primary vein and in the angles between the primary and secondaries, although
expression varies between sun and shade leaves.

**Inference for this recipe.** Choose a mature summer leaf as the primary
recipe state. Its edge should be clean at game scale, with no bright hair
fringe. Use moderate upper-surface gloss variation, a lighter and less glossy
reverse, and slightly stronger underside vein relief. If a young-leaf variant
is later needed, encode ciliation as a bounded edge roughness/coverage response
or separate close-card variant rather than sprinkling white lines across the
mature recipe. Age, damage, and season should be explicit variant controls, not
unstructured noise in the species master.

### Sun and shade leaves justify bounded physical variation

**Evidence.** The Biological Flora review reports mean beech-leaf thickness
falling from about 197.5 micrometres at the top of the crown to about 99.7
micrometres at the base. A dedicated surface study likewise documents canopy-
height differences in beech thickness, cuticle, and epidermal cell walls
([Fernández et al. 2018: surface properties of *Fagus sylvatica*
leaves](https://pmc.ncbi.nlm.nih.gov/articles/PMC5915543/)).

**Inference for this recipe.** A single exact thickness or transmission value
would be false precision. Support a small sun/shade family: upper-canopy leaves
may be thicker, darker, and less transmissive; lower-canopy leaves thinner,
lighter, and more transmissive. Preserve one species silhouette and vein
grammar. Do not use random hue noise as a proxy for these correlated physical
differences.

## What procedural foliage artists do

### Establish controllable structure before secondary variation

**Evidence.** SideFX's procedural-leaf lesson starts from the basic shape,
adds explicit shape controllers and rough displacement, then applies bounded
size/shape variation when leaves are planted on a stem. Damage such as insect
bites is an optional later layer
([SideFX: Creating a Procedural Leaf
Recipe](https://www.sidefx.com/tutorials/creating-a-procedural-leaf-recipe-in-houdini-intermediate-tutorial/)).
Material artist Ilana Katz describes starting with a reusable vein network,
deriving different leaf forms from shared rules, and using the veins to drive
height, color cells, and roughness variation
([80 Level: Making a Leaf Generator in Substance
Designer](https://80.lv/articles/making-a-leaf-generator-in-substance-designer)).
Jonathan Benainous likewise separates the input silhouette mask from exposed
color, age, curvature, and damage controls
([Procedural Leaf Generator](https://jonathan_benainous.artstation.com/projects/ko2gz)).

**Inference for this recipe.** A dedicated beech module should expose semantic
fields—blade mask, edge distance, midrib, secondary index/distance, tertiary
links, upper/reverse identity, and optional age state. Compose output maps from
those fields. This is preferable to one opaque function where changing margin
frequency inadvertently changes width or where albedo and relief redraw veins
independently.

### Houdini practice separates leaf prototype, placement, and instancing

**Evidence.** The SideFX Labs Simple Leaf keeps length, width, profile, center
fold, bend, point jitter, and color as separate prototype controls
([SideFX Labs Tree Simple Leaf](https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_simple_leaf.html)).
The Labs Tree Leaf Generator separately controls distribution, orientation,
deformation, atlas variants, normals, and packed instancing
([SideFX Labs Tree Leaf Generator](https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_leaf_generator.html)).

A SideFX forum discussion with production practitioners recommends nested
packed primitives for leaf/tree/forest reuse and notes that ordinary polygons
and bump maps are sufficient for leaves except at close range
([SideFX forum: Instancing
Instances](https://www.sidefx.com/forum/topic/31254/)). Another SideFX forum
thread records a real failure in which a leaf generator reset custom spherical
normals, underscoring that normal ownership must be tested across the complete
placement tool rather than assumed from the prototype
([SideFX forum: Tree Leaf Generator resets leaf
normal](https://www.sidefx.com/forum/topic/77770/)).

**Inference for this recipe.** The texture should own millimetre-scale veins,
surface cells, and cuticle response. A small card mesh should own the center
fold, shallow transverse corrugation, cup, and tip droop. Placement should own
orientation and canopy distribution. Use a small set of reusable prototype
cards/texture variants and instance them; do not generate a unique texture per
leaf. Verify that runtime placement preserves authored or canopy-adjusted
normals.

### Scans are strongest as measured references, not unexplained baked lighting

**Evidence.** Traditional production tree-texture workflows use photographs,
scanned botanical illustrations, or painted elements and emphasize clean alpha
extraction; photographs require removal of captured lighting, while botanical
illustrations offer diffuse separation
([Game Developer, February 2002: tree texture
methods](https://media.gdcvault.com/GD_Mag_Archives/GDM_February_2002.pdf)).
Modern procedural artists commonly accept imported or drawn masks while keeping
color, relief, damage, and variants procedural, as in Benainous's generator.

**Inference for this recipe.** Photograph multiple ordinary *F. sylvatica*
leaves from front, reverse, grazing, and transmitted-light views. Use them to
measure ranges for length/width, maximum-width position, petiole ratio, vein
angles, margin-wave amplitude, front/reverse value, and thickness response.
Keep the shipped result deterministic and source-image-free if that is the
project direction. Never copy a single specimen's shadow, specular highlight,
disease, or tear into every procedural leaf.

## Relief, translucency, and two-sided response

### Put broad fold in geometry and fine relief in maps

**Evidence.** SideFX separates profile, folding, bending, and jitter in its
simple leaf mesh. Its procedural-leaf tutorial adds rough displacement after
the fundamental shape. The foliage practitioners above build height from a
vein system rather than adding unrelated noise.

**Inference for this recipe.** Use card geometry for a shallow center fold,
small cup, and perhaps one lengthwise bend. Use height/normals for the tapered
midrib, secondaries, subtle intercostal corrugation, and restrained cuticle
breakup. Beech should not look hammered, leathery, or bark-like. On the upper
surface, broad panels can be almost smooth and glossy; on the reverse, veins
should be more prominent without becoming thick cords.

### Transmission needs a thickness field, not emissive green

**Evidence.** The real-time SpeedTree rendering work uses tangents and normal
maps for dynamically lit cards and a two-sided model. It explicitly observes
that backlit leaves are dominated by transmitted rather than reflected light,
with a yellow/red hue shift
([GPU Gems 3: Next-Generation SpeedTree
Rendering](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering)).
Guerrilla's *Horizon Zero Dawn* vegetation pipeline stores albedo, tangent-space
normal, alpha, translucency, masks, and AO as separate authored channels
([GDC Vault: Between Tech and Art: The Vegetation of *Horizon Zero
Dawn*](https://www.gdcvault.com/play/1025530/), [GDC 2018
slides](https://media.gdcvault.com/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf)).

**Inference for this recipe.** Preserve distinct front/reverse albedo and
normal maps. When the runtime material supports it, derive a transmission or
thickness map from anatomy: lamina transmits most, secondaries less, and midrib
and petiole least. Modulate the range for sun/shade variants. Do not brighten
the entire reverse uniformly or bake a permanent sun-facing gradient into
albedo.

## Alpha coverage, mips, and temporal stability

### Coverage-aware mips are a required part of the leaf, not optional polish

**Evidence.** Guerrilla's GDC presentation describes a custom mip workflow:
measure source alpha-test coverage, build regular mips, bilinearly upsample for
coverage histograms, then rescale alpha around the runtime 0.5 cutoff so the
original coverage is retained. Ignacio Castaño describes the same production
failure—ordinary filtering changes the fraction of texels that survive alpha
test and makes foliage shrink or disappear—and solves it by matching each
mip's coverage at the actual cutoff
([Castaño: Computing Alpha
Mipmaps](https://www.ludicon.com/castano/blog/articles/computing-alpha-mipmaps/)).

**Inference for this recipe.** The current absence of mips is not a safe way to
protect the shallow beech margin: it trades controlled simplification for
aliasing and unstable sampling. Generate a complete semantic mip chain and
preserve threshold coverage at the runtime cutoff. Track more than total area:

- broad ovate/elliptic silhouette occupancy;
- survival of the short petiole while resolvable;
- amplitude and phase of the few readable margin waves;
- open transparent card corners;
- continuity of the midrib and large secondaries; and
- absence of dark or green halos from transparent texels.

Very shallow crenations should intentionally disappear when subpixel. Their
stable disappearance is preferable to keeping them with maximum filtering,
which would dilate the blade and eventually fill the card.

### Motion tests are different evidence from still texture exports

**Evidence.** GPU Gems reports that hard alpha-cutout vegetation edges
scintillate under wind animation and that alpha-to-coverage substantially
reduces the artifact. Wyman and McGuire show that fixed alpha testing can lose
aggregate appearance under minification; hashed alpha replaces the fixed
threshold with a stable procedural threshold to reduce disappearance without
adding more temporal flicker than conventional alpha test
([Wyman and McGuire: Hashed Alpha
Testing](https://casual-effects.com/research/Wyman2017Hashed/index.html)).

**Inference for this recipe.** Evaluate consecutive frames during fractional-
pixel camera movement, slow distance change across mip boundaries, card sway,
and grazing rotation. Still exports establish silhouette and channel coherence,
but cannot establish temporal stability. Test coverage-correct mips with the
game's actual antialiasing first; hashed alpha may be unnecessary for beech's
broad, smooth silhouette and can introduce objectionable noise.

## Recommended construction

1. **Create a dedicated beech module.** Keep the stable recipe ID and output
   interface, but replace the generic sampler with species-owned silhouette,
   venation, relief, and tests.
2. **Build a calm asymmetric envelope.** Use an ovate/elliptic blade with a
   short petiole, acute tip, and slight independent left/right variation.
3. **Author seven or eight secondary pairs.** Give them bounded individual
   origin, angle, curvature, and reach. Let the vein graph determine whether a
   margin segment is entire, wavy, or weakly blunt-toothed.
4. **Keep the basal margin quiet.** Concentrate the optional crenation toward
   the middle and upper blade; avoid uniform sawtooth repetition.
5. **Separate material scales.** Put whole-leaf fold/cup in card geometry;
   midrib, secondaries, and subtle panels in height/normals; cuticle and mature
   gloss in roughness.
6. **Author both faces.** Keep the reverse lighter and less glossy, strengthen
   underside vein relief, and reserve a vein-aware thickness/transmission map
   for the runtime material.
7. **Use a bounded sun/shade family.** Correlate thickness, value, gloss, and
   transmission. Keep damage and young ciliation as explicit optional variants.
8. **Generate complete coverage-aware mips.** Solve against the real material
   cutoff, filter covered colors without transparent-black bleed, and
   renormalize normal mips.
9. **Instance a few card variants.** Preserve prototype normals through the
   placement path and review clusters/canopies as well as an isolated leaf.

## Deterministic tests and visual acceptance

### Species geometry

- The blade is ovate to elliptic, with declared bounds for length/width,
  maximum-width position, petiole/length, base width, and tip angle.
- Left and right margins differ modestly but remain one coherent blade.
- There are seven or eight main secondary pairs in the default variant.
- Each major secondary has an explicit edge relationship: it loops near an
  entire margin or supports one shallow blunt crenation.
- The basal third is less toothed than the middle/upper blade.
- The margin never becomes deeply serrate, lobed, perfectly periodic, or a set
  of isolated one-pixel teeth.

### Channels and anatomy

- Repeated generation is byte-identical.
- Opacity, front/back albedo, front/back normals, height, and ARM all contain
  the complete declared mip chain.
- Covered albedo remains within the approved small palette; transparent texels
  do not introduce black fringes during filtering.
- Reverse albedo is measurably lighter than front albedo. Metallic remains zero
  and roughness stays within declared mature-leaf bounds.
- Height cross-sections rank midrib above secondaries and secondaries above
  tertiary/intercostal detail. Fine structure cannot exceed the amplitude of
  the anatomy that contains it.
- Every normal is finite and normalized. Front and reverse tangent conventions
  are verified on an actual two-sided card under rotating light.
- A future thickness map ranks lamina above secondaries above midrib/petiole for
  transmission, with separate bounded sun/shade ranges.

### Coverage and temporal behavior

- Report alpha-test coverage error for every mip at the exact runtime cutoff.
- Shallow margin waves simplify monotonically rather than flickering or growing
  under minification.
- Card corners remain transparent through all mips where an individual leaf is
  still the intended representation.
- Consecutive capture frames cover subpixel translation, gradual zoom through
  mip boundaries, card sway, and grazing angles.
- Capture isolated front, isolated reverse, backlit leaf, three-card cluster,
  and canopy-distance views; pair lit captures with opacity/normal diagnostics.
- Fail acceptance on card-shaped fill, black halos, unstable occupancy, lost
  midrib while the blade is still large, inverted reverse normals, or glossy
  plastic response.

## Pitfalls to avoid

- A generic symmetric oval with eight identical sine-wave teeth.
- Treating European beech like the more sharply serrate American beech or
  hornbeam.
- Drawing teeth and secondaries from unrelated functions.
- Perfectly straight, identical veins with no taper or near-margin behavior.
- Equal front/back relief and a constant color offset as the only side
  difference.
- White hair strokes on every mature leaf.
- High-frequency RGB noise standing in for sun/shade or age variation.
- No mips, ordinary box-filter alpha, or maximum-alpha mips without measured
  cutoff coverage.
- Baking card-scale fold into normals while retaining a perfectly flat grazing
  silhouette.
- Reviewing one hero leaf only; repetition, normal loss, and alpha instability
  appear in clusters and in motion.

## Bottom line

The current beech parameters have a plausible broad envelope and the correct
default count of eight secondary pairs, but the generic implementation misses
the relationship that makes *Fagus sylvatica* recognizable: a quiet undulating
margin organized by a strong regular venation rhythm. The highest-value next
step is a dedicated analytic beech recipe in which the vein graph owns both
surface structure and shallow edge events. Pair that with deliberately distinct
front/reverse response, a small correlated sun/shade family, a few instanced
folded cards, and complete cutoff-aware mips. Final acceptance must include
consecutive motion through mip transitions; still texture-lab images alone
cannot prove that a thin, glossy beech canopy will remain stable.
