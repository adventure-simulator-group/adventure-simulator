# Dry white-oak leaf prior art

## Scope and naming

This report concerns only the `DryWhiteOakLeaf` texture recipe. In repository
terms it is the senescent/dry state of the shared pedunculate-oak leaf form,
not a separate species generator. The code's "white oak" name should therefore
be read as the white-oak lineage containing European pedunculate oak
(*Quercus robur*), rather than as a claim that the depicted leaf is the North
American species commonly called white oak (*Quercus alba*).

The target is a deterministic, inexpensive two-sided leaf-card material for a
1544 German environment. It needs to work both as retained dry foliage on an
oak and as litter/scatter input, while remaining visually identifiable as the
same oak leaf used by the living recipe.

## Repository facts and constraints

The following are observed repository constraints, not findings from the
external sources.

- The living and dry states share one analytic pedunculate-oak silhouette and
  vein layout: five uneven rounded lobe pairs, auriculate base, short petiole,
  midrib, and lobe-directed secondary veins.
- Drying currently changes material response and relief but deliberately leaves
  the base opacity image byte-identical to the living state.
- The 256 by 256 recipe emits opacity, front albedo, back albedo, front normal,
  back normal, height, and packed AO/roughness/metallic channels, each with nine
  mips.
- The dry palette currently has exactly two front-face colors: brown lamina and
  darker veins. The reverse is lighter. Roughness is 236/255 and metallic is
  zero.
- Dry relief attenuates the living relief, adds broad margin lift, and adds two
  low-amplitude sinusoidal pucker fields. The normal maps are derived from that
  height response.
- Opacity mips use a maximum of the four source texels, a deliberately
  conservative rule. Color mips remain in the authored palette by selecting the
  covered source color nearest their mean rather than inventing blended colors.
- Runtime use is on cards. The texture recipe owns material-scale shape cues;
  leaf placement, card geometry, wind, and canopy normals belong elsewhere.

## Practitioner workflows

### Scan or author one leaf, extract semantic maps, then make controlled variants

**Evidence.** Traditional game-foliage practice starts from photographs,
scanned illustrations, or painted leaves and spends substantial effort on a
clean alpha channel, because detailed leaf and branch structures are more
economical in a texture than in geometry. Botanical illustrations can be useful
because they are accurate, diffusely lit, and easy to separate from their
background; photographs require harder masking and removal of lighting
([Game Developer, February 2002: tree texture
methods](https://media.gdcvault.com/GD_Mag_Archives/GDM_February_2002.pdf)).

Modern photometric workflows extend this into explicit material channels. One
documented practitioner setup photographs a leaf under eight light directions,
derives a normal map in Substance 3D Sampler, cleans and expands an opacity
selection, and exports albedo, normal, and opacity maps
([PolyCG: Photometric scan texture
maps](https://www.polycg.com/post/create-realistic-rose-flower-with-photometric-scan-textures-chapter-1-making-a-textures-map)).
Research on physically based real-time leaves uses separate scans of the upper
and lower surfaces, aligns them, makes simplified geometry, and bakes normal,
displacement, thickness, and albedo data; it warns that specular highlights may
need removing from measured albedo ([Physically Based Real-Time Translucency
for Leaves](https://studyres.com/doc/20348584/physically-based-real-time-translucency-for-leaves)).

**Inference for this recipe.** The current fully analytic workflow is valid, but
its outputs should correspond to those same semantic observations: clean
species opacity, distinct upper/lower albedo, mesostructural normal/height,
roughness, and eventually thickness/transmission if the renderer supports it.
Reference scans should inform distributions and scale, not be copied into the
shipping texture. The recipe can remain deterministic and source-image-free.

### Artists separate leaf identity from deformation and placement

**Evidence.** SideFX's procedural-leaf lesson builds the basic leaf shape,
exposes shape controls, adds rough displacement, and then creates planted
variations in shape and size; insect bites are treated as an optional variation,
not as the core identity ([SideFX: Creating a Procedural Leaf
Recipe](https://www.sidefx.com/tutorials/creating-a-procedural-leaf-recipe-in-houdini-intermediate-tutorial/)).
The Labs Tree Leaf Generator accepts either simple internal cards or custom leaf
inputs, supports atlas variants, and independently controls scale, orientation,
pruning, curl noise, secondary anti-aliased noise, tropisms, normals, and packed
instancing ([SideFX Labs Tree Leaf Generator
documentation](https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_leaf_generator.html)).

SideFX's tree-generator course similarly bakes detailed designs to a single
polygon for background trees or reuses them as branches in a 3D tree
([SideFX: Tree Generator](https://www.sidefx.com/tutorials/tree-generator/)).

**Inference for this recipe.** Preserve the shared species silhouette and
venation. Put senescence-specific pigmentation and sub-card relief in this
recipe, but do not bake placement yaw, whole-card folding, gravity, or wind into
the maps. A small family of deterministic material variants can share the same
species boundary while card geometry supplies larger C-curl/S-curl/fold states.

### Dry-leaf materials benefit from simulation-informed folds, but can be baked

**Evidence.** Technical artist Michael Ekker traces leaf alpha cards into
geometry, remeshes and preserves atlas UVs, then runs a Houdini Vellum
simulation to obtain natural pile-up, crinkles, and folds. The result is baked
to a tiled material; a front/back color ID permits separate reverse-side
treatment, and dirt and dampness are added afterward in Substance Designer
([Michael Ekker: Fallen Leaves](https://www.michaelekker.com/projects/3de8Bo)).
A separate leaf-decay demonstration by Adobe senior technical artist Maximilien
Vert uses Substance Designer for animated material change and Houdini Vellum for
mesh deformation
([80.lv: Leaf Decay Animation](https://80.lv/articles/impressive-leaf-decay-animation-made-with-houdini-substance-3d-designer)).

**Inference for this recipe.** A height-derived normal can carry fine puckering,
vein relief, and crisp dry wrinkles, but broad curling should eventually be a
few card-mesh variants. Baking a simulated reference into normals is useful for
visual study, yet the runtime recipe can approximate it analytically if its
frequency, amplitude, and vein anchoring match the reference.

## Botanical constraints on the dry state

### Identity should remain pedunculate oak

**Evidence.** The Woodland Trust describes *Quercus robur* leaves as simple and
alternate, with four to seven pairs of uneven lobes, two basal lobes, and a very
short stalk ([Woodland Trust Nature's Calendar: Pedunculate
oak](https://naturescalendar.woodlandtrust.org.uk/what-we-record-and-why/species-we-record/trees/oak-pedunculate/)).
ICP Forests records autumn *Q. robur* leaves as progressing through yellow,
yellow-brown, orange-brown, and brown ([ICP Forests: British oak phenological
phases](https://icp-forests.org/documentation/Annex/Phenological_phases/British_oak.html)).

**Inference for this recipe.** Keeping living and dry opacity identical is a
sound species contract. Damage, missing lobes, and torn margins may be separate
variants, but should not be mandatory consequences of drying.

### Brown is a family of retained pigments, not a uniform dark overlay

**Evidence.** University of Missouri Extension attributes brown oak and beech
autumn color to brownish tannin compounds combined with carotenoids, while
weather, light, drought, frost, and the timing of chlorophyll loss alter the
result. Oaks can therefore pass through yellow-brown, bronze, orange-brown, and
brown states rather than one universal hue ([MU Extension: Autumn
Colors](https://extension.missouri.edu/publications/g5010)). Oaks can also
retain dead leaves through winter; the International Oak Society discusses
marcescence and includes *Q. robur* among the Old World oaks displaying it
([International Oak Society: When Oak Leaves Fail to
Fall](https://www.internationaloaksociety.org/content/when-oak-leaves-fail-fall)).

**Inference for this recipe.** A restrained ochre/umber/bronze palette is more
credible than a neutral gray-brown or saturated orange. Variation should follow
leaf structure and drying exposure: paler reverse, darker persistent veins,
some edge/tip darkening, and a few broad low-frequency pigment zones. Do not use
independent high-frequency RGB noise; it reads as stone or compression rather
than senescent tissue.

If the project's small solid-palette contract remains firm, use four to six
curated swatches selected by deterministic masks: base lamina, warm retained
carotenoid patch, deeper tannin-brown patch, dark vein/petiole, and a slightly
lighter/desaturated underside. Preserve those exact swatches in the mip chain.
The current two-color front is coherent but likely too molded and uniform for a
close individual leaf.

### Drying curl is constrained by the midvein

**Evidence.** A 2026 mechanics study combines observations, simulation, and
theory to show that a shrinking lamina constrained by a comparatively
non-shrinking midvein develops curling- or folding-dominated shapes. It reports
both C- and S-curls, with C-curls more common in the modeled regime, and folds
accompanied by edge waviness. The relative bending stiffness of lamina and
midvein controls the outcome ([Guo et al.: Midveins regulate the shape formation
of drying leaves](https://www.sciencedirect.com/science/article/pii/S0022509625003655)).
An earlier morphometric study finds that primary vein frameworks strengthen the
long axis, so dried shape change is expressed mainly as narrowing; dehydration
shrinks cells and reduces total leaf area ([Tung et al.: Is Shape of a Fresh and
Dried Leaf the Same?](https://pmc.ncbi.nlm.nih.gov/articles/PMC4821626/)).

**Inference for this recipe.** Replace purely free-floating sinusoidal pucker
with a strain-inspired field:

1. keep the midrib comparatively stiff and low-curvature locally;
2. increase curl magnitude with lateral distance from the midrib;
3. allow one broad signed longitudinal mode for C-curl, with an occasional
   deterministic S-curl variant;
4. add edge waviness whose amplitude grows toward lobe margins but is damped at
   secondary veins; and
5. introduce modest transverse narrowing only in optional dry geometry, not by
   mutating the shared opacity contract.

The current broad margin lift is directionally plausible. Its two global sine
fields are cheap, but their phase should be subordinated to midrib and lobe
coordinates so they read as constrained lamina buckling rather than embossed
fabric.

## Runtime-card and shading lessons

### Two-sided response is material information, not merely flipped geometry

**Evidence.** SpeedTree's real-time rendering work uses per-pixel dynamic
lighting on leaf cards with tangents and normal maps and emphasizes two-sided
leaf lighting. Backlit leaves are dominated by transmitted rather than reflected
light ([GPU Gems 3: Next-Generation SpeedTree
Rendering](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering)).
Guerrilla's *Horizon Zero Dawn* vegetation pipeline stores alpha, tangent-space
normal, albedo, translucency, mask, and AO data, and explicitly handles
double-sided tangent-space normals. It also colorizes vegetation through
artist-driven masks rather than replacing the underlying albedo indiscriminately
([GDC Vault: Between Tech and Art: The Vegetation of Horizon Zero
Dawn](https://www.gdcvault.com/play/1025530/), [Guerrilla GDC 2018
slides](https://ubm-twvideo01.s3.amazonaws.com/o1/vault/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf)).

**Inference for this recipe.** Keep separately authored front and back albedo
and normals. The reverse should not simply invert every normal channel or add a
constant brightness offset without a rendered tangent-space check. Dry leaves
should be highly rough and weakly specular, but still benefit from modest
backlighting/transmission where tissue remains thin. The material should not
look like opaque brown plastic.

### Opacity mips must preserve coverage without turning cards into blobs

**Evidence.** Castaño documents that ordinary box-filtered alpha changes the
fraction of texels passing an alpha test, causing foliage to thin and disappear.
He rescales each mip to approximate the base level's coverage at the actual
runtime cutoff ([Ignacio Castaño: Computing Alpha
Mipmaps](https://www.ludicon.com/castano/blog/articles/computing-alpha-mipmaps/)).
Guerrilla's production approach likewise measures source coverage and rescales
each mip's histogram around its 0.5 alpha-test value. The same presentation
warns that alpha textures need good anti-aliasing.

**Inference for this recipe.** The current max-of-four opacity reduction
guarantees survival but is conservative dilation, not measured coverage
preservation. It can close the sinuses between oak lobes and eventually turn a
leaf into a rectangular or oval blob. Generate opacity mips against the actual
alpha cutoff, preserve integrated lobe/sinus coverage where representable, and
record the point at which individual lobes become sub-pixel. Color mips should
remain premultiplied or coverage-aware so black transparent texels never bleed
into brown edges.

### Card economy needs controlled variation, not unique textures everywhere

**Evidence.** SideFX's leaf generator supports packing/instancing for large
trees and atlas variants for visual diversity. SpeedTree supports both flat
cards and 3D leaf meshes, using normal maps and two-sided shading to recover
material detail cheaply. Ekker's fallen-leaf workflow likewise reuses atlas
cards, then obtains variety through remeshing, simulation, scatter, and material
masks.

**Inference for this recipe.** One species atlas can provide a small number of
dry-state variants: flat/lightly cupped, C-curled, S-curled, and folded. Reuse a
single underlying opacity and palette family. Vary card roll, scale, and
orientation at placement time. This is cheaper and more legible than procedural
per-instance texture generation, and more convincing than repeating one flat
card at every angle.

## Recommended recipe direction

1. **Keep the species mask authoritative.** Continue deriving living and dry
   states from the same pedunculate-oak lobe and venation model.
2. **Add structured senescence masks.** Build two or three low-frequency zones
   from distance to edge, distance along the blade, and vein influence. Use them
   to select a small historically/botanically plausible ochre-to-umber palette.
3. **Keep vein coloration coherent.** Midrib and secondary veins should remain
   legible but not become uniformly black outlines. Let the underside veins be
   slightly lighter/raised and the upper veins darker or less saturated.
4. **Make relief anatomical.** Construct height from midrib, secondary veins,
   shallow interveinal panels, and a margin-curl field. Modulate dry pucker by
   distance to structural veins and lobe edges.
5. **Reserve broad curl for geometry variants.** Height/normal should convey
   millimetre-scale pucker; the card mesh should carry centimetre-scale C/S curl
   and folds. A completely flat silhouette with only a strong normal map will
   fail at grazing angles and in shadow.
6. **Use measured alpha coverage.** Replace maximum-filter opacity mips with a
   cutoff-aware coverage chain, or prove with rendered captures that sinus
   closure remains acceptable at every used mip.
7. **Preserve palette semantics in mips.** The current nearest-authored-color
   reduction is a good stylized constraint. If more swatches are added, keep
   that categorical behavior while filtering coverage separately.
8. **Plan for two-sided lighting.** Maintain independent front/back maps and
   expose a thin-tissue transmission or translucency channel when the runtime
   material contract can consume it.

## Deterministic tests and visual acceptance

- Dry and living mip-zero opacity remain identical unless a separately named
  damage variant is introduced.
- The silhouette retains four to seven recognizable uneven lobe pairs, basal
  auricles, and the short petiole at review scale.
- Every nontransparent albedo texel belongs to the declared dry palette; no
  black fringe enters color mips.
- Front and reverse palettes are distinct but related, with the reverse lighter
  and less saturated within bounded values.
- Roughness stays high and metallic stays zero.
- The height field is continuous at the blade edge, and its strongest broad
  curl grows away from the midrib rather than crossing it arbitrarily.
- Secondary-vein neighborhoods damp or redirect pucker; margins show more
  waviness than the central lamina.
- Normal maps are normalized through every mip and independently verified on a
  double-sided runtime card.
- Alpha-test coverage is measured at the runtime cutoff for every used mip.
  Sinus closure, petiole loss, and conversion to a solid card are explicit
  failure conditions.
- Captures include front, reverse, backlit, grazing, and ground-litter views,
  plus consecutive motion frames at mip transitions.
- At least three cards are shown together. This reveals mirrored repetition,
  identical curl phase, and value clustering that a single hero card hides.
- Comparison includes the living oak leaf beside the dry leaf to prove shared
  species identity and genuinely different material/relief response.

## Pitfalls to avoid

- Treating "dry" as a uniform brown multiply over the living texture.
- Importing photographic highlights or hard shadows into albedo.
- Using unrestricted RGB noise that breaks the small-palette art direction.
- Making every lobe edge equally dark; this reads as an illustration outline.
- Driving curl with texture-space sine waves unrelated to the midrib, veins, or
  margins.
- Encoding a large whole-leaf fold only in normals; its silhouette and shadow
  will remain flat.
- Using maximum alpha filtering without checking that lobe sinuses stay open.
- Reusing the front normal or color unchanged on the reverse.
- Making all dry leaves identical. Variation should be a small, controlled set
  of reusable material/card states, not per-instance noise.
- Confusing autumn-tinted attached leaves, fully dry marcescent leaves, damp
  litter, and decomposing litter. They are related states with different value,
  roughness, curl, and contamination, not one texture.

## Bottom line

The current architecture makes the right high-level choice: drying changes a
shared pedunculate-oak leaf instead of inventing another silhouette. The next
quality gain should come from structured, limited senescence coloration and
midvein-constrained relief. Keep fine pucker in height/normals, move broad curl
to a few reusable card meshes, retain separate upper/lower responses, and
replace conservative opacity dilation with cutoff-aware coverage validation.
That combination follows both production foliage practice and the mechanics of
real drying leaves without sacrificing deterministic generation or runtime
economy.
