# Living white-oak leaf prior art

## Scope and species contract

This report concerns only the living `WhiteOakLeaf` recipe. Repository comments
identify its form as pedunculate oak, *Quercus robur*, the common European oak
appropriate to the game's German setting. The internal "white oak" name should
therefore be understood as a lineage/category name, not as the North American
species *Quercus alba*.

The target is a deterministic, species-readable, two-sided leaf-card texture.
This report covers its procedural opacity, venation, relief, material response,
runtime-card behavior, mip chain, and visual acceptance. Branch architecture,
whole-tree growth, wind, and placement remain outside the recipe except where
they establish a card/material interface.

## Repository facts and constraints

The following are facts observed in the repository, not claims from the cited
sources.

- The living and dry recipes share one dedicated analytic *Q. robur* silhouette
  and vein layout. Drying changes relief and palette, not species identity.
- Five independent rounded lobe contributions are authored on each side, plus a
  terminal contribution. Left and right lobe centers, radii, and reach differ.
- A narrow continuous lamina connects the lobes across sinuses. The base has
  auriculate lobes around a very short petiole.
- Each authored lobe receives a curved secondary vein, with a short fork used as
  restrained tertiary structure. The midrib tapers toward the tip.
- Relief combines a transverse blade dome, longitudinal dome, vein ridges,
  alternating interveinal corrugation, and low-amplitude tissue mottle.
- A 4 by 4 sub-texel majority sample generates binary mip-zero opacity at 256
  by 256.
- Outputs are opacity, front/back albedo, front/back tangent-space normals,
  height, and packed AO/roughness/metallic, each with nine mips.
- Front blade `[76, 111, 48]`, vein `[139, 157, 76]`, and reverse blade
  `[91, 116, 65]` form a small solid palette. Continuous pigment mottle is
  quantized to one adjacent darker value. Roughness is 219/255 and metallic is
  zero.
- Opacity mips take the maximum of four source texels, ensuring survival but
  dilating the silhouette. Color mips select an existing covered palette color
  near the local mean; normal mips renormalize their average.
- The texture is intended for runtime cards. Whole-card bend, placement,
  canopy/vertex normals, animation, and LOD are separate owners.

## Species-shaped mask construction

### Rounded irregular lobes, auricles, and a short petiole are the primary read

**Evidence.** Kew describes *Q. robur* leaves as dark green above, pale green
below, with three to six lobes and a particularly short stalk ([Kew: Oak tree,
*Quercus robur*](https://www.kew.org/plants/oak-tree)). Oregon State describes
three to six pairs of deep rounded lobes, little basal "ear lobes," a short
petiole, and dark-green/blue-green upper/lower coloration
([Oregon State University: *Quercus
robur*](https://landscapeplants.oregonstate.edu/plants/quercus-robur)). Flora of
China records five to seven rounded or retuse lobes per side, an auriculate base,
a 2–5 mm petiole, and five to seven, sometimes ten, secondary veins per side
([Flora of China: *Quercus
robur*](https://efloras.org/florataxon.aspx?flora_id=2&taxon_id=210001863)).

The Botanical Society of Scotland emphasizes that *Q. robur* has rounder and
fewer lobes than sessile oak, an auriculate base, a very short petiole, and
generally has intercalary veins between lobes ([Botanical Society of Scotland:
*Quercus robur*](https://botsoc.scot/2021/11/14/plant-of-the-week-november-9th-2021-quercus-robur-l-the-common-oak-the-english-oak-or-the-pedunculate-oak/)).

**Inference for this recipe.** The current explicit lobe family, unequal sides,
auricles, and short petiole are the correct semantic model. Preserve the
continuous lamina so sinuses remain broad and rounded rather than cutting all
the way to the midrib. Variation should adjust lobe center, asymmetric
proximal/distal radii, reach, and sinus depth within bounds. Avoid a generic
`sin(n*t)` lobe modulation: it produces identical spacing and mirrored lobes,
which is unlike the observed irregular form.

The most species-critical automated comparisons are against sessile oak:
retain the auriculate base and short petiole, keep lobes relatively few and
rounded, and permit intercalary veins. A procedural oak that loses those traits
may still read as "oak" but not reliably as *Q. robur*.

### The blade should be broader toward the upper-middle, not uniformly elliptical

**Evidence.** Flora descriptions range from obovate to obovate-oblong. The New
Zealand flora calls the adult blade narrowly to broadly obovate and describes
three to seven pairs of obtuse lobes with an auriculate base
([Flora of New Zealand: *Quercus
robur*](https://floraseries.landcareresearch.co.nz/taxa/33c4fa06-52c8-48f9-bb3e-4b5bdbf150aa)).
Morphological comparisons between pedunculate and sessile oak use petiole
ratio, lobe position, sinus depth, secondary-vein length, intercalary veins, and
base shape as discriminants ([Kremer et al.: Differences in leaf morphology
between *Q. petraea* and *Q.
robur*](https://silvafennica.fi/pdf/268)).

**Inference for this recipe.** Test not only lobe count but the distribution of
width along the axis: the broadest region should generally sit above the basal
third, while the auricles remain a distinct basal event rather than the widest
part. Compare a family of measured ratios—length/width, petiole/length, basal
lobe width, maximum-width position, and several sinus depths—rather than tuning
to one idealized leaf.

## Vein hierarchy and relief

### Lobe-directed and intercalary veins should coexist

**Evidence.** Flora of China reports five to seven (occasionally ten)
secondaries per side. The Botanical Society of Scotland notes that *Q. robur*
generally has intercalary veins between lobes. A morphological study identifies
the length of the second side vein and number of veins between lobes as useful
species characters. General oak venation studies report secondaries extending
toward lobes and additional tertiary structures around sinuses.

**Inference for this recipe.** Keep one major curved secondary for each dominant
lobe. Add a smaller, lower-contrast intercalary vein in selected sinuses, ending
before or at the sinus rather than inventing another lobe. Build the hierarchy
as an explicit graph:

1. tapered midrib from petiole to terminal lobe;
2. lobe-directed secondaries with unequal origins and reach;
3. selected intercalary veins associated with deep sinuses;
4. short forks from secondaries; and
5. only very faint reticulate breakup at close-map scale.

The graph should drive albedo masks, height, normals, AO, and any future
thickness/transmission—not be redrawn independently per channel. This follows a
common material-artist pattern: Blizzard principal material artist Eric Wiley
began procedural maple, oak, and birch generators with a small vein graph,
placed it along the stem under shared curve control, and generated distinct top
and bottom height maps ([Eric Wiley: Fallen Autumn
Leaves](https://wiley3d.artstation.com/projects/YvnmY)).

### Living relief is shallow corrugation, not bark-like embossing

**Evidence.** Botanical descriptions characterize the blade as membranous and
largely glabrous at maturity; upper and lower values differ, and lower veins may
retain sparse hairs. The leaf is not a thick sculpted object. Procedural artists
commonly expose rough displacement as a controlled layer after establishing the
basic shape and veins ([SideFX: Creating a Procedural Leaf
Recipe](https://www.sidefx.com/tutorials/creating-a-procedural-leaf-recipe-in-houdini-intermediate-tutorial/)).

**Inference for this recipe.** Keep a broad, low dome and alternating
interveinal corrugation, but constrain them to the vein graph. Vein ridges should
taper and soften with order; tertiary forks must not carry the same relief as
the midrib. Fine tissue mottle should remain lower amplitude than all structural
veins and should not create high-frequency specular sparkle. A useful physical
scale split is:

- card geometry: centimetre-scale cup, fold, and twist;
- height/normal: millimetre-scale midrib, secondary veins, and corrugation;
- roughness/transmission: sub-millimetre cuticle and tissue response.

## Scan versus procedural synthesis

### Scan data is best used as measured reference and channel ground truth

**Evidence.** Traditional game foliage uses photographs, scanned botanical
illustrations, or painted elements; illustrations offer diffuse light and clean
separation, while photos demand careful alpha extraction and removal of baked
lighting ([Game Developer, February 2002: tree texture
methods](https://media.gdcvault.com/GD_Mag_Archives/GDM_February_2002.pdf)). A
documented photometric workflow photographs a leaf under multiple light
directions, derives a normal map, cleans and expands the opacity selection, and
exports albedo, normal, and opacity channels ([PolyCG: Photometric scan texture
maps](https://www.polycg.com/post/create-realistic-rose-flower-with-photometric-scan-textures-chapter-1-making-a-textures-map)).

Physically based leaf acquisition separately scans upper and lower surfaces,
aligns geometry, simplifies it, and derives top/bottom albedo, normals,
displacement, and thickness. The workflow notes that captured specular
highlights may need removal from albedo ([Physically Based Real-Time
Translucency for Leaves](https://studyres.com/doc/20348584/physically-based-real-time-translucency-for-leaves)).

**Inference for this recipe.** Retain source-image-free deterministic synthesis,
but build a reference board from several healthy *Q. robur* leaves photographed
front, reverse, transmitted, and grazing. Measure ranges, not pixels: lobe
positions, sinus depths, petiole ratio, front/reverse value, vein widths, and
corrugation scale. This avoids inheriting photography shadows and preserves the
small-palette direction while grounding the algorithm in real specimens.

### Mature procedural tools expose semantic masks rather than one opaque graph

**Evidence.** Adobe senior technical artist Maximilien Vert's leaf generator
creates multiple venation types, exposes age/damage/color controls, and applies
the generated textures to a simply deformed plane ([Maximilien Vert: Leaf
Generator](https://maximilienvert.artstation.com/projects/qA0znz)). Lead
material artist Jonathan Benainous uses an input mask to drive a reusable leaf
graph, allowing waveforms, painted masks, or imported images to define shape;
color, age, curvature-based effects, and insect damage remain separate controls
([Jonathan Benainous: Procedural Leaf
Generator](https://jonathan_benainous.artstation.com/projects/ko2gz)).

**Inference for this recipe.** Keep distinct intermediate fields for silhouette,
midrib, lobe secondaries, intercalary veins, tissue, edge distance, and
front/back identity. Compose output channels from those fields. This makes
species iteration independently testable and prevents a later color change from
silently changing relief or opacity.

## Living tissue, translucency, and two-sided response

### Backlighting is not ordinary reverse-side diffuse lighting

**Evidence.** SpeedTree's real-time rendering work uses tangents and normal maps
for dynamic per-pixel leaf-card lighting and an explicit two-sided model. It
observes that when leaves are lit from behind, transmitted light rather than
reflected light is the major contribution ([GPU Gems 3: Next-Generation
SpeedTree Rendering](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering)).

Guerrilla's *Horizon Zero Dawn* vegetation pipeline stores alpha, tangent-space
normal, albedo, translucency amount, masks, and AO, and handles double-sided
tangent-space normals. Vegetation colorization is mask-driven and artist
controlled rather than a global replacement ([GDC Vault: Between Tech and Art:
The Vegetation of Horizon Zero
Dawn](https://www.gdcvault.com/play/1025530/), [Guerrilla GDC 2018
slides](https://ubm-twvideo01.s3.amazonaws.com/o1/vault/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf)).

**Inference for this recipe.** The existing front/reverse albedo and normals are
the right foundation. The reverse should be lighter and less saturated, as the
current palette provides. When the runtime material supports it, add a
thickness/transmission field derived from vein distance: lamina transmits more,
major veins and petiole less. This is preferable to a uniform emissive-like
backlight. Verify tangent-space reversal on actual cards; channel inversion by
inspection is not sufficient.

### A small palette can still describe living tissue

**Evidence.** Guerrilla separates an underlying albedo from artist-driven
colorization masks. Substance leaf generators likewise expose age and color as
semantic controls. These workflows preserve structure while permitting bounded
art direction.

**Inference for this recipe.** The current two adjacent blade colors plus a vein
color suit the molded-material aesthetic, but placement of those colors should
follow broad pigment/tissue fields rather than uniform random speckle. Candidate
structured variation includes slightly darker upper interveinal panels, lighter
young tissue near some veins, and a paler reverse. Keep the total palette small
and exact through the mip chain. Do not paint black AO into albedo or use bright
yellow outlines around every vein.

## Runtime cards and economy

### Geometry, material detail, and placement have different jobs

**Evidence.** The SideFX Labs Tree Leaf Generator can generate simple cards or
accept custom leaf inputs; it separately controls deformation, curl noise,
orientation, normals, atlas variants, and packed instancing
([SideFX Labs Tree Leaf Generator
documentation](https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_leaf_generator.html)).
SideFX's tree-generator course bakes tree designs to a single polygon for
background use or reuses them as branches ([SideFX: Tree
Generator](https://www.sidefx.com/tutorials/tree-generator/)). A SideFX forum
question about an oak with 150,000 leaves illustrates why flat quads, instancing,
and multiple resolutions matter even outside real-time games ([SideFX forum:
Tree leaf geometry
instancing](https://www.sidefx.com/forum/topic/20950/?page=1)).

**Inference for this recipe.** Use opacity for lobe silhouette, normals/height
for veins and shallow corrugation, a low-vertex card for broad cup/fold, and the
placement system for orientation and canopy volume. Produce a small reusable
set of card variants rather than unique textures. A flat quad cannot reproduce
a folded grazing silhouette; a highly tessellated lobe mesh wastes geometry at
ordinary canopy distance.

## Alpha mips, distance, and temporal stability

### Coverage preservation must be measured at the runtime cutoff

**Evidence.** Castaño documents how ordinary alpha mipmaps change the fraction
of fragments passing the alpha test, making foliage thin and disappear. His
production solution measures base coverage at the actual cutoff and rescales
successive mips to approximate it ([Ignacio Castaño: Computing Alpha
Mipmaps](https://www.ludicon.com/castano/blog/articles/computing-alpha-mipmaps/)).
Guerrilla similarly generated a custom mip chain by measuring source coverage,
building regular mips, bilinearly upsampling for histograms, and scaling the
relevant histogram point to a 0.5 cutoff.

**Inference for this recipe.** The current max-of-four rule guarantees that the
leaf survives, but systematically dilates lobes, fills sinuses, thickens the
petiole, and can ultimately expose the card. Replace it with cutoff-aware
coverage preservation or prove its error acceptably bounded in the actual
renderer. Track not only total occupancy but also:

- number and spacing of readable lobe runs;
- sinus openness while projected size permits;
- auricle and short-petiole readability;
- card-corner fill; and
- front/back edge-color halos.

The smallest mips cannot retain lobe topology. Define an intentional transition
to a simpler aggregate leaf/canopy representation rather than letting maximum
filtering silently choose a solid shape.

### Motion acceptance is distinct from still-image acceptance

**Evidence.** The SpeedTree chapter notes that hard alpha cutout edges
scintillate under animation and reports alpha-to-coverage as reducing that
artifact. Wyman and McGuire show that fixed alpha testing loses aggregate
appearance at coarse sampling; hashed alpha improves spatial and temporal
stability, though high-quality temporal supersampling is still important
([Wyman and McGuire: Hashed Alpha
Testing](https://research.nvidia.com/sites/default/files/pubs/2017-02_Hashed-Alpha-Testing/Wyman2017Hashed.pdf)).

**Inference for this recipe.** Require consecutive frames during fractional-
pixel camera movement, card sway, and mip transitions. Texture-lab stills can
establish botanical form and channel coherence but cannot prove the lack of
shimmer. Test coverage-correct mips with the game's actual anti-aliasing first;
hashed alpha is likely unnecessary for broad oak lobes and may introduce noise
that reads as damaged tissue.

## Recommended construction

1. **Retain explicit asymmetric lobes.** Keep rounded, unequal lobe primitives
   joined by a continuous lamina. Constrain count, reach, and sinus depth to a
   measured *Q. robur* family.
2. **Protect the auriculate base and petiole.** These are high-value species
   identifiers and should have dedicated tests at mip zero and card distance.
3. **Build a shared venation graph.** Midrib, lobe-directed secondaries,
   selected intercalary veins, and restrained forks should drive color, relief,
   AO, and future thickness.
4. **Separate relief scales.** Broad cup in card geometry; corrugation, veins,
   and tissue dome in height/normals; cuticle softness in roughness.
5. **Author two sides deliberately.** Keep the reverse paler and make its major
   veins more structurally legible. Add vein-aware transmission when supported.
6. **Preserve the small palette.** Use deterministic structural masks, not RGB
   noise. Keep transparent texels from bleeding black into filtered edges.
7. **Generate cutoff-aware mips.** Solve coverage at the real material cutoff,
   report residual error, and establish the distance where individual lobes are
   intentionally ceded to a simpler LOD.
8. **Review clusters as well as hero cards.** A few card variants with distinct
   cup/fold and slight material variation reveal repetition and canopy-lighting
   problems that an isolated leaf cannot.

## Deterministic tests and visual acceptance

### Species geometry

- Mip-zero silhouette contains the expected rounded asymmetric lobe family,
  continuous lamina, auriculate base, terminal lobe, and very short petiole.
- Lobe and sinus sequences are not perfect mirrors; neither side self-intersects
  or leaves isolated pixel islands.
- Width/length, maximum-width position, petiole/length, basal-ear width, and
  representative sinus-depth ratios stay within declared bounds.
- Every dominant lobe has a secondary vein; selected sinuses have smaller
  intercalary veins without accidental extra lobes.

### Channels

- Generation is byte-identical across runs and all seven outputs contain all
  nine mips.
- Every covered albedo texel belongs to the declared palette, and transparent
  black does not contaminate filtered edges.
- Reverse albedo remains measurably lighter than front albedo without becoming
  chalky.
- Metallic remains zero and roughness remains in the bounded living-leaf range.
- Normal vectors are finite and normalized in every mip. Front and reverse are
  verified on a double-sided runtime card under rotating directional light.
- Height cross-sections show the midrib strongest, secondaries weaker, forks
  weaker again, and tissue mottle lowest.
- Any transmission map is thickest/darkest at petiole and major veins and is
  tested against backlit reference renders.

### Coverage and motion

- Supersampled mip-zero opacity agrees with a higher-sample reference around
  lobe tips, sinuses, auricles, terminal lobe, and petiole.
- Threshold coverage is reported for every render-used mip at the actual alpha
  cutoff.
- Mip reduction does not monotonically expand the leaf into its card rectangle.
- Consecutive captures cover sub-pixel translation, card sway, gradual distance,
  at least one mip transition, and a grazing view.
- Diagnostic captures isolate opacity/crown-like silhouette from lighting.
  Fail on large unexplained occupancy oscillation, premature sinus closure,
  petiole flicker while still resolvable, black halos, or solid-card fill.
- Visual boards include front, reverse, transmitted, grazing, three-card
  cluster, and canopy-distance views, plus the dry state beside the living leaf
  to prove shared identity and different material response.

## Pitfalls to avoid

- Calling any generic lobed leaf "oak" without auricles and a short petiole.
- Perfectly periodic lobes or bilateral mirroring.
- Cutting sinuses too deeply, producing a pin-oak-like or skeletal silhouette.
- Omitting intercalary veins entirely or giving every small fork equal visual
  weight.
- Independent random masks per channel; structure must remain coherent.
- Embossed vein lines of uniform width and height.
- Uniform two-sided color or normals with no transmitted-light plan.
- Baking highlights and cast shadows from scan reference into albedo.
- Maximum alpha mips accepted solely because the final mip remains opaque.
- Judging only stationary hero-card stills.
- Encoding whole-leaf fold in normals while leaving silhouette and shadow flat.
- Generating unique textures for every leaf instead of a small instanced variant
  set.

## Bottom line

The current living oak implementation has the correct high-level representation:
explicit asymmetric rounded lobes, continuous sinuses, auriculate base, very
short petiole, lobe-directed veins, separate two-sided maps, and restrained
palette variation. The most valuable next improvements are species-specific
intercalary veins, more rigorously hierarchical relief, vein-aware
translucency, and cutoff-aware opacity mips. Keep lobe identity in the analytic
mask, whole-leaf cup in cheap card geometry, and require consecutive-motion
captures before accepting its distance behavior.
