# BlackthornLeaf procedural prior art

Internal research draft for the `BlackthornLeaf` acceptance cycle. This is a
technique survey, not public project prose and not a substitute for blackthorn
botanical ground truth.

## Question

Which practitioner techniques for procedural leaf construction and real-time
foliage transfer cleanly into Fabelgeist's deterministic 256 px, seven-channel,
alpha-card recipe, and which conflict with its molded-material palette, shared
runtime material, or ownership boundaries?

## Inspected practitioner sources

### SideFX and Houdini

1. SideFX, “Labs Tree Simple Leaf” documentation:
   https://www.sidefx.com/docs/houdini/nodes/sop/labs--tree_simple_leaf.html
   - Source class: official tool documentation, inspected 2026-09-01.
   - The leaf tool separates length/width, outline profile, fold profile, bend
     profile, point jitter/seed, and color/stem controls.
   - Transferable principle: keep species silhouette, blade relief, and instance
     deformation as separate parameter families. In this repository the texture
     sampler owns silhouette/vein relief, while the production cambered mesh and
     seeded plant generator own bending and per-instance variation.
   - Constraint: its longitudinal color ramps and free point jitter do not
     transfer directly. Fabelgeist requires hard-bounded solid albedo regions
     and deterministic parameters; runtime geometry already supplies seeded
     variation.
2. Chetal Gazdar, “Creating a Procedural Leaf Recipe,” SideFX tutorial, 17 March
   2023:
   https://www.sidefx.com/tutorials/creating-a-procedural-leaf-recipe-in-houdini-intermediate-tutorial/
   - Source class: practitioner tutorial hosted by SideFX, inspected 2026-09-01.
   - The workflow starts from a controllable basic shape, adds rough
     displacement, then creates planted shape/size variation and optional insect
     damage.
   - Transferable principle: author a clean species base and add relief through
     a distinct channel after the outline is stable.
   - Constraint: bug bites and per-leaf damage are deliberately excluded. They
     would add unsupported history to the base recipe and violate the current
     art-direction rule against baking damage or grime into albedo.
3. SideFX forum, “Trees and leaves,” 18–24 November 2017:
   https://www.sidefx.com/forum/topic/52677/?page=1
   - Source class: practitioner forum exchange, inspected 2026-09-01.
   - The concrete workflow routes color and alpha as separate textures onto a
     leaf plane.
   - Transferable principle: opacity is an independent authored signal rather
     than color luminance. Fabelgeist already follows this with distinct opacity
     and front/back albedo handles and should preserve exact alpha parity.
   - Constraint: this thread demonstrates a basic material hookup, not species
     construction, mip behavior, PBR response, or a measured performance result.
4. SideFX forum, “Using Texture Atlases within Houdini for materials,” 26 June
   2017: https://www.sidefx.com/forum/topic/50486/
   - Source class: practitioner forum exchange, inspected 2026-09-01.
   - A suggested workflow traces an atlas leaf silhouette into geometry and
     orthographically projects UVs while preserving the source aspect ratio.
   - Transferable principle: silhouette, card aspect, and UV footprint must
     agree; empty card area and an incorrect physical aspect can undermine an
     otherwise good alpha.
   - Constraint: Fabelgeist generates a single analytic leaf map and uses shared
     procedural card meshes rather than importing a multi-leaf atlas. Geometry
     tracing is therefore a review insight, not an implementation recipe for
     this task.

### GDC production material

5. Gilbert Sanders, Guerrilla Games, “Between Tech and Art: The Vegetation of
   Horizon Zero Dawn,” GDC 2018: https://www.gdcvault.com/play/1025530/ and
   slides at
   https://media.gdcvault.com/gdc2018/presentations/gilbert_sanders_between_tech_and.pdf
   - Source class: first-party production conference talk and slides, inspected
     2026-09-01.
   - The talk describes signed-distance alpha textures, a custom mip-chain
     coverage algorithm that measures base alpha-test coverage and rescales each
     mip to preserve it at a 0.5 test threshold, compact 256 × 128 alpha
     textures for small assets/grass, and vegetation channel sets including
     alpha, tangent-space normal, albedo, translucency, mask, and AO.
   - Transferable principle: alpha mip generation must be evaluated against the
     actual cutoff and preserve apparent coverage; distant teeth must neither
     vanish nor inflate. Independent alpha, normal, albedo, AO, and transmission
     concepts support the repository's channel-separated material.
   - Constraint: Fabelgeist currently uses binary analytic alpha and a shared
     max-coverage mip reducer rather than SDF alpha, has a 0.34 blackthorn
     cutoff, and cannot adopt Guerrilla's whole depth/geometry/shader pipeline
     within one texture task. The reported PS4 workload and texture layout are
     not a comparable performance ceiling.

### Credible procedural-art breakdowns

6. “Procedural Vegetation Materials Breakdown,” 80 Level, 2017:
   https://80.lv/articles/procedural-vegetation-materials-breakdown
   - Source class: artist workflow breakdown, inspected 2026-09-01.
   - The artist decomposes plants into reusable pieces, constructs a leaf/stalk
     alpha, builds the central vein from simple transformed/blended shapes, then
     adds bumps and veins as surface detail before assembling an atlas.
   - Transferable principle: use explicit analytic masks for blade, petiole,
     midrib, secondary veins, and relief instead of one undifferentiated noise
     field. The blackthorn sampler follows this decomposition.
   - Constraint: the breakdown later adds gradients, water drops, dirt, and
     damage. Those operations are not transferable to Fabelgeist's base albedo
     because the current art direction prohibits baked lighting, stains, dirt,
     and wear and limits albedo to hard solid-color regions.
7. “Guide: Procedural Plant Growth in Houdini,” 80 Level / Creative Bloq
   summary, 24 October 2017:
   https://80.lv/articles/guide-procedural-plant-growth-in-houdini
   - Source class: practitioner tutorial summary, inspected 2026-09-01.
   - The workflow begins from nature reference, draws the outer leaf curve, uses
     low-poly single-sided geometry, remeshes, and lifts the center near the
     stem.
   - Transferable principle: reference-driven outer shape and a bounded central
     lift are higher-value than generic noise. The candidate uses an explicit
     side-width profile and midrib/blade relief.
   - Constraint: Fabelgeist requires distinct front/back appearance and a
     double-sided runtime material, so “single-sided” is an efficiency
     observation rather than a rendering contract for this repository.
8. Ignacio Castaño, “Computing Alpha Mipmaps” for *The Witness*, 9 September
   2010: https://www.ludicon.com/castano/blog/articles/computing-alpha-mipmaps/
   - Source class: first-party engine programmer production breakdown, inspected
     2026-09-01.
   - Standard filtered mipmaps changed the proportion of texels passing alpha
     test and made foliage fade. Castaño describes measuring base coverage at
     the application cutoff and finding a per-mip alpha scale by bounded search
     so coverage remains as close as possible to the base.
   - Transferable principle: mip validation must measure cutoff coverage, not
     merely require a nonzero final mip. This corroborates the independent GDC
     production technique.
   - Constraint: the shared Fabelgeist `LeafMipSemantic::Coverage` currently
     takes the maximum of each 2 × 2 block, which is conservative against
     disappearance but can expand coverage. Replacing it globally would affect
     accepted leaf recipes and is outside BlackthornLeaf ownership; production
     temporal capture must therefore treat both disappearance and inflation as
     explicit gates.

## Technique transfer matrix

| Technique | Evidence | Transfer | Decision for this candidate |
|---|---|---|---|
| Separate outline/profile from fold, bend, and jitter | SideFX Simple Leaf | Strong | Keep outline and relief in the species sampler; leave instance bend/variation in production geometry. |
| Reference-driven silhouette curve | SideFX/80 Level | Strong | Use explicit elliptic-obovate side profiles and a rounded basal boundary. |
| Explicit blade, petiole, midrib, secondary-vein masks | 80 Level breakdown | Strong | Retain analytic semantic regions and derive normal/height/AO from them. |
| Rough displacement after shape | SideFX tutorial | Strong with limits | Use bounded tissue relief only in height/normal; do not contaminate albedo. |
| Separate color and opacity maps | SideFX forum | Strong | Preserve opacity as its own texture and test alpha equality across albedo mips. |
| Shape/UV/card aspect agreement | SideFX atlas forum | Strong as a review gate | Inspect production card proportions; do not trace or add an atlas in this task. |
| SDF alpha | Guerrilla GDC | Conditional | Do not add a new alpha representation in this bounded task; note as a future shared-system alternative. |
| Cutoff-aware coverage-preserving mips | Guerrilla GDC; The Witness | Strong, but shared-system scoped | Do not change the shared reducer here. Add coverage-by-mip evidence and reject visual inflation/disappearance in temporal capture. |
| Random damage, bug bites, dirt, gradients | SideFX/80 Level | Conflicts | Exclude from the living base recipe and its albedo. |
| Per-leaf procedural variation inside the texture | SideFX tools | Weak here | Keep one deterministic species texture; production seeded geometry controls variation. |

## Candidate audit caused by this research

- Keep the dedicated analytic `blackthorn_leaf` module: it follows the
  practitioner pattern of a controlled species profile followed by separate
  relief and channel derivation.
- Keep front albedo to two hard solid colors and back albedo to two hard solid
  colors. Reject practitioner gradient/dirt/damage operations because they
  conflict with current project art direction.
- Keep the central dome and vein relief bounded. The stronger back-vein relief
  is the texture-level cue for source-backed underside hair without inventing
  albedo speckle.
- Do not alter the shared card mesh, material binding layout, SDF
  representation, or global mip generator in this task.
- Strengthen acceptance evidence: record per-mip alpha-test coverage at the
  production cutoff of 0.34, compare it with base coverage, and visually inspect
  consecutive production frames for both tooth disappearance and silhouette
  inflation. A final nonzero mip alone is insufficient.
- Treat production card aspect and the separately recorded 35–64 mm physical
  size as review inputs. Do not compensate for a geometry-scale issue by
  distorting the texture.

## Remaining uncertainty

No inspected practitioner source provides a blackthorn-specific vein-count,
BRDF, roughness, transmission, or pubescence-normal amplitude. Those remain
botanical/art-direction judgments bounded by the existing material and
independent visual review. Guerrilla's and *The Witness*'s cutoff-preserving
algorithms establish a technique direction, not a directly comparable
performance or visual target for Bevy/WebGPU.
