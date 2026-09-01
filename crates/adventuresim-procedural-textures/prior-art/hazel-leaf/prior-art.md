# Hazel leaf prior art

## Scope

This report concerns only the `HazelLeaf` recipe for common European hazel,
*Corylus avellana*. The target is a deterministic, species-readable two-sided
leaf-card material for the game's European environment. It does not prescribe
branch generation, whole-shrub architecture, wind, or leaf placement, except
where those systems impose a texture or runtime-card contract.

Hazel is a good test of whether a procedural leaf system preserves botanical
hierarchy. Its broad cordate blade can look generic at first glance, but the
combination of a narrow basal notch, abruptly acuminate tip, coarse double
serration, and secondary veins ending in the larger teeth is distinctive. Those
relationships matter more than adding arbitrary noise.

## Repository facts and constraints

The following are current repository facts, not conclusions drawn from prior
art.

- `hazel_leaf.rs` owns a dedicated analytic silhouette, vein field, height,
  normals, and material maps at 256 by 256 with nine mips.
- The blade is broad and almost round through most of its length, has a narrow
  cordate basal notch, and contracts abruptly into an acuminate tip.
- Each side has eight independently authored secondary veins. A larger tooth is
  centered at each vein terminus, with two smaller shoulder teeth, producing an
  asymmetric double-serrate margin.
- Relief combines a blade dome, longitudinal dome, raised midrib/secondaries,
  alternating corrugation beside the secondary veins, and two small sinusoidal
  tissue-relief terms.
- Four by four sub-texel sampling produces a binary mip-zero opacity mask.
- The recipe emits opacity, front/back albedo, front/back normals, height, and
  packed AO/roughness/metallic. Front albedo has exactly two solid colors:
  lamina `[66, 112, 48]` and vein `[129, 154, 75]`; the reverse lamina is lighter
  `[82, 119, 61]`. Roughness is 216/255 and metallic is zero.
- Opacity mips take the maximum of each 2 by 2 source block. This guarantees
  survival but dilates the mask. Color mips select an existing covered palette
  color nearest the local mean. Normal mips renormalize their average.
- The texture is consumed by runtime cards. Card geometry, atlas layout,
  placement, vertex normals, animation, and LOD policy are separate systems.

## Species evidence and procedural implications

### The silhouette is broad, cordate, abruptly pointed, and doubly serrate

**Evidence.** A modern Biological Flora account describes *Corylus avellana*
leaves as broadly ovate to elliptic, 5–12 cm long and 4–12 cm wide, with a
narrowly cordate to rounded base, abruptly acuminate apex, and coarsely doubly
serrate margin. It records seven to eight, occasionally nine, pairs of primary
secondary veins ([Hicks 2023, Biological Flora of Britain and Ireland:
*Corylus avellana*](https://besjournals.onlinelibrary.wiley.com/doi/full/10.1111/1365-2745.14008)).
Trees and Shrubs Online independently describes a rounded to obovate blade,
narrowly cordate base, abrupt acuminate tip, and coarsely double-toothed or
slightly lobulate margin ([Trees and Shrubs Online: *Corylus
avellana*](https://www.treesandshrubsonline.org/articles/corylus/corylus-avellana/)).

**Inference for this recipe.** The current broad envelope, basal notch, sudden
tip contraction, and eight vein pairs are well grounded. Preserve them as
semantic parameters. Do not synthesize the margin with a single high-frequency
sine wave: primary teeth should remain tied to secondary-vein endpoints, with
smaller secondary teeth inserted between them. Mild left/right phase and size
asymmetry is plausible; arbitrary global skew that erases the cordate base is
not.

### Venation and relief should form one construction

**Evidence.** The Biological Flora account states that the midrib and secondary
veins are sunken on the upper surface and prominent below, that secondaries
terminate at the margin, and that tertiary veins are scalariform with a
reticulate finer network. It also notes simple white hairs on both surfaces,
especially tufts in vein axils, and stronger underside hair presence. A leaf-
venation analysis specifically observes that basal secondary veins in *C.
avellana* originate virtually at the lamina base ([Coomes et al.: The
Mathematical Treatment of Leaf
Venation](https://pmc.ncbi.nlm.nih.gov/articles/PMC4241078/)).

**Inference for this recipe.** Generate the large-tooth locations from the
secondary-vein graph, as the code already does, and derive mesoscopic relief
from the same graph:

1. a strong midrib with thickness tapering toward the tip;
2. seven to nine secondary ridges reaching primary teeth;
3. shallow intercostal panels that alternately rise and fall between adjacent
   secondaries;
4. very faint scalariform/reticulate breakup below card-hero scale; and
5. separate upper and lower normal response, with more prominent veins below.

The source says upper veins are sunken, not raised. The current shared positive
vein ridge on both faces should therefore be judged carefully in rendered
front/reverse captures. A practical stylization may keep a slight upper ridge
for readability, but it should be identified as an art-direction choice rather
than botanical reconstruction. The reverse should carry the stronger relief.

### Hairiness is a roughness and value cue, not necessarily explicit fibers

**Evidence.** Both botanical references report fine pubescence, and the
Biological Flora account locates simple hairs on upper and lower lamina, petiole,
and vein axils, with more visible underside expression. University College Cork
summarizes hazel leaves as hairy and soft to the touch
([UCC Tree Explorers: *Corylus
avellana*](https://www.ucc.ie/en/tree-explorers/trees/a-z/corylusavellana/)).

**Inference for this recipe.** At 256 pixels and ordinary card distances,
individual trichome geometry or white hair strokes would alias and make the
leaf look diseased. Represent pubescence through high but not chalky roughness,
a slightly lighter/desaturated reverse, subdued specular response, and perhaps
a deterministic low-amplitude microsurface term confined to the underside and
vein axils. Keep it below the frequency of the double teeth.

## What procedural foliage artists do

### Start with a controllable species recipe, then add bounded variation

**Evidence.** SideFX's procedural-leaf lesson builds the basic leaf shape,
exposes shape controls, adds rough displacement, and creates variations in
planted shape and size. Insect bites are an optional variation layered on the
leaf rather than the basis of its silhouette ([SideFX: Creating a Procedural
Leaf Recipe](https://www.sidefx.com/tutorials/creating-a-procedural-leaf-recipe-in-houdini-intermediate-tutorial/)).

The SideFX Labs Tree Leaf Generator accepts generated cards or custom leaf
inputs and treats atlas variants, scale, orientation, pruning, deformation,
curl noise, anti-aliased secondary noise, normals, and instancing as separate
controls ([SideFX Labs Tree Leaf Generator
documentation](https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_leaf_generator.html)).

**Inference for this recipe.** Keep species identity in deterministic functions:
envelope, cordate base, vein origins, vein-tooth correspondence, and tip. Limit
variation to small parameter families—width, notch depth, tooth amplitude,
vein phase, hue/value, and card cupping—rather than injecting independent noise
at every stage. Generate several reusable variants once; do not create a unique
texture per leaf instance.

### Scan and synthesis are complementary

**Evidence.** An established foliage workflow starts from photographs, scanned
botanical illustrations, or painted elements, then extracts a clean alpha;
illustrations offer diffuse lighting and easy separation while photos require
careful masking and lighting removal ([Game Developer, February 2002: tree
texture methods](https://media.gdcvault.com/GD_Mag_Archives/GDM_February_2002.pdf)).
A photometric practitioner workflow uses multiple light directions to recover
normal response, then cleans/expands the opacity selection and exports albedo,
normal, and opacity ([PolyCG: Photometric scan texture
maps](https://www.polycg.com/post/create-realistic-rose-flower-with-photometric-scan-textures-chapter-1-making-a-textures-map)).

SideFX's production-oriented foliage course begins from reference photographs,
uses Houdini to make procedural tools, and targets real-time meshes rather than
abstract procedural demonstrations ([SideFX: Create Foliage for
Games](https://www.sidefx.com/tutorials/create-foliage-for-games/)).

**Inference for this recipe.** Use backlit or cross-lit hazel scans as
measurement references for tooth scale, vein prominence, interveinal sag,
front/back value difference, and roughness. Keep generation analytic so the
result is deterministic and palette-bounded. A useful validation board should
place generated opacity and relief beside several independent specimens; it
should not tune to a single unusually round, narrow, damaged, or cultivated
leaf.

### Cards recover volume through geometry, normals, and distribution

**Evidence.** SideFX's tree course bakes detailed tree designs to a single
polygon for background use or reuses them as branch assets
([SideFX: Tree Generator](https://www.sidefx.com/tutorials/tree-generator/)).
The Labs leaf tool supports packed instancing and atlas variants because large
trees cannot afford unique high-detail geometry per leaf. SpeedTree uses both
flat cards and 3D leaf meshes, with tangents and normal maps for dynamic
per-pixel lighting and an explicit two-sided leaf model ([GPU Gems 3:
Next-Generation SpeedTree
Rendering](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-4-next-generation-speedtree-rendering)).

**Inference for this recipe.** The texture's normal and height should describe
millimetre-scale vein relief and corrugation. Whole-leaf cupping, folding along
the midrib, and petiole angle should live in a small card mesh or vertex
deformation. A flat quad with an aggressive normal map still has a flat grazing
silhouette and shadow. Conversely, tessellating every tooth is wasteful at
canopy distance; alpha is the correct owner of double serration until a close
hero leaf proves otherwise.

## Alpha coverage, distance, and temporal stability

### Ordinary alpha mips lose fine teeth; maximum mips overgrow them

**Evidence.** Castaño describes the familiar production failure: ordinary mip
averaging changes the fraction of texels passing the alpha test, causing leaf
silhouettes to thin and disappear. His solution measures base coverage at the
runtime cutoff and scales subsequent mip alpha to preserve that coverage
([Ignacio Castaño: Computing Alpha
Mipmaps](https://www.ludicon.com/castano/blog/articles/computing-alpha-mipmaps/)).

Guerrilla used an offline custom coverage pipeline for *Horizon Zero Dawn*:
calculate source coverage, build ordinary mips, bilinearly upsample each mip for
a histogram, find the point corresponding to original coverage, and rescale it
to the runtime 0.5 threshold. Their vegetation textures separately carry alpha,
normal, albedo, translucency, masks, and AO, and the presentation emphasizes
that good anti-aliasing is still required ([GDC Vault: Between Tech and Art: The
Vegetation of Horizon Zero
Dawn](https://www.gdcvault.com/play/1025530/), [Guerrilla GDC 2018
slides](https://ubm-twvideo01.s3.amazonaws.com/o1/vault/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf)).

**Inference for this recipe.** Maximum reduction is a defensible temporary
conservative choice for a thin petiole and teeth, but it is not neutral: every
mip expands opaque regions, fills the cordate notch, closes serration sinuses,
and eventually produces a solid blob. Replace or validate it with cutoff-aware
coverage preservation. Track feature survival separately:

- whole-blade occupancy;
- openness of the basal notch;
- projected count/amplitude of primary teeth;
- survival of the petiole; and
- degree of rectangular card fill.

Once secondary teeth project below a pixel, their disappearance is acceptable;
flickering between present and absent is not. Primary-tooth rhythm and broad
cordate/acuminate identity should survive farther.

### Temporal evidence requires motion, not still endpoints

**Evidence.** The SpeedTree rendering chapter notes that harsh alpha-cutout
edges scintillate under animation and that alpha-to-coverage substantially
reduces this while remaining order-independent. Wyman and McGuire show that
fixed alpha testing loses aggregate appearance at coarse resolution; hashed
alpha improves spatial and temporal stability, although high-quality temporal
supersampling remains important ([Wyman and McGuire: Hashed Alpha
Testing](https://research.nvidia.com/sites/default/files/pubs/2017-02_Hashed-Alpha-Testing/Wyman2017Hashed.pdf)).

**Inference for this recipe.** Evaluate consecutive frames while camera and leaf
move by fractions of a pixel. Stationary texture-lab images can establish
species shape and map coherence, but not shimmer. The temporal harness should
isolate a leaf/card mask and measure occupancy changes around mip transitions,
especially at the pointed teeth and petiole. Hashed alpha is probably excessive
for a regular, readable single-leaf card; coverage-correct mips plus the game's
actual AA path should be tested first.

## Recommended construction

1. **Define the leaf in blade coordinates.** Use a broad ovate/suborbicular
   envelope, explicit cordate notch, and abrupt acuminate tip. Keep left and
   right sides independently parameterized within narrow botanical bounds.
2. **Author the vein graph first.** Place seven to nine secondary pairs from
   near the blade base toward the tip. Taper their reach and thickness.
3. **Generate double serration from venation.** Put one coarse primary tooth at
   each secondary terminus and one or two smaller teeth in the interval. Vary
   sizes and spacing modestly without breaking the correspondence.
4. **Build anatomical relief.** Use midrib and secondary distance fields,
   intercostal panels, and very low-amplitude tissue undulation. Make the upper
   veins shallow/sunken or only subtly raised; make reverse veins more prominent.
5. **Keep a small palette.** The current blade/vein split is compatible with a
   stylized procedural system. If variation is needed, add only a few authored
   greens selected by deterministic broad masks; do not introduce noisy RGB
   speckle. Keep the reverse lighter and less saturated.
6. **Represent pubescence cheaply.** Favor roughness, reverse-side value, and
   restrained microsurface response over explicit hair lines.
7. **Generate coverage-aware mips.** Solve against the material's actual alpha
   cutoff and record residual occupancy, notch openness, and primary-tooth
   survival. Filter colors without transparent-black bleed and renormalize
   normals.
8. **Use several card variants.** A flat/slightly cupped card and one or two
   folded variants are enough. Placement owns orientation and distribution;
   atlas choice owns limited visual variation.

## Applicable deterministic tests

### Species and topology

- Blade width/length stays in the broad near-orbicular range supported by the
  botanical descriptions.
- The center of the basal notch remains outside while both basal lobes remain
  inside.
- The tip narrows abruptly rather than tapering like beech or blackthorn.
- There are seven to nine secondary-vein pairs and every one reaches a primary
  margin tooth.
- Each interval contains smaller secondary teeth, and left/right tooth phases
  are not perfectly mirrored.
- Tooth ordering never self-intersects or creates isolated one-texel islands.

### Channels

- Generation is byte-for-byte deterministic and all outputs contain complete
  mip chains.
- Every covered albedo texel belongs to the declared small palette.
- Front and reverse values are distinct and their roles are stable through mips.
- Metallic is zero; roughness stays within the authored non-glossy leaf range.
- Every normal mip is normalized, finite, and consistent with the height map's
  gradient convention.
- Reverse vein relief is at least as legible as upper relief in rendered
  directional-light comparisons.

### Coverage and motion

- Mip-zero supersampled opacity agrees with a higher-sample reference around
  the notch, teeth, tip, and petiole.
- Threshold coverage error is reported for every used mip at the real runtime
  cutoff.
- Primary serration does not grow monotonically outward under minification;
  secondary teeth may simplify but the card must not become a solid rectangle.
- Consecutive capture frames span sub-pixel translation, gradual distance
  change, a mip transition, and a grazing angle.
- Captures include isolated front, isolated reverse, backlit, three-card cluster,
  and canopy-distance views. Review the opacity diagnostic beside the lit image.
- Fail on large unexplained frame-to-frame occupancy oscillation, black edge
  halos, lost cordate notch while the leaf is still large on screen, or card-
  shaped fill.

## Pitfalls to avoid

- A generic oval with uniform sawtooth noise.
- Perfectly periodic or perfectly mirrored teeth.
- Secondary veins that stop in the lamina instead of reaching primary teeth.
- Equal-amplitude relief unrelated to the vein graph.
- Making the upper and lower surfaces differ only by a constant color offset.
- Drawing individual white hairs at a scale that aliases in motion.
- Maximum-filtering opacity without checking notch closure and silhouette
  dilation.
- Preserving every tiny secondary tooth farther than screen resolution permits;
  this causes shimmer rather than species fidelity.
- Baking whole-card cup/fold into normals while leaving the silhouette flat.
- Judging a single isolated hero leaf only; repetition and distance failures
  emerge in clusters.

## Bottom line

The existing hazel recipe has the right semantic skeleton and unusually good
botanical alignment: broad cordate blade, abrupt point, eight secondary pairs,
and primary teeth generated at vein termini. The highest-value improvements are
to make upper/lower relief more anatomically distinct, subordinate fine tissue
variation to the vein graph, and replace conservative maximum opacity mips with
cutoff-aware coverage behavior. Keep double serration in alpha at card scale,
put broad cupping in a few reusable card meshes, and require consecutive-motion
captures before calling its distance rendering stable.
