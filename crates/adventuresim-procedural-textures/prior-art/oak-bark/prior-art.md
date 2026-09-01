# Pedunculate-oak bark prior art

## Scope

This report concerns exactly the `OakBark` procedural texture and its runtime
use on the project's oak trunks and branches. The species target is pedunculate
or English oak, *Quercus robur*, which is native to central Europe and already
named `ENGLISH_OAK_BARK` in repository tests. It covers the intrinsic bark
substrate, age and trunk-scale variation, optional damp/moss/lichen overlays,
projection, channel causality, physical scale, filtering, LOD handoff, and
acceptance tests. Tree growth, branch silhouettes, leaves, and forest placement
are outside scope except where they supply the bark material with radius,
branch order, orientation, or distance.

The intended end product is not generic brown "tree noise." At normal play
distance a mature trunk should read as grey-brown, thick bark divided into
short, narrow, mostly vertical plates by branching, longitudinal, deep
fissures. At close range the plate shoulders, lifted crowns, shorter checks,
and restrained fibrous breakup should explain the normal, occlusion, and
roughness response. Young axes should transition toward smooth silver-grey
bark rather than carrying a miniaturized copy of the mature trunk.

## Repository facts and constraints

Everything in this section is observed in the repository. It is not an
external-source claim.

- `OakBark` is an implemented `Wood` recipe whose only generated image is a
  packed `height-ao` texture. It is `RG8Unorm`, 1024 by 1024, repeating, and has
  a complete eleven-level mip chain.
- One tile represents 0.5 m square. Mip zero therefore samples approximately
  0.488 mm per texel. The declared height range is 0.032 m. The generator clamps
  the normalized field to `[-0.5, 0.32]`, encodes `height + 0.5`, and the shader
  decodes `(sample.r - 0.5) * 0.032`; the currently reachable relief is thus
  asymmetric, approximately -16 mm to +10.24 mm.
- The current height construction uses 38 smoothly blended plate sites, an
  explicit graph of short fissure edges, tapered checks, sparse fibre groups,
  plate crowns, shoulders, broad breakup, and fine breakup. Its comment states
  the core semantic contract: continuous longitudinal furrows, smoothly
  blended crown variation, and subordinate checks that taper before they can
  outline closed cells.
- Broad AO is derived from the same metric height field at 512 by 512 using
  four directions and steps of 1, 4, 12, and 32 AO texels. Full-resolution local
  cavity visibility is multiplied into that result. The resulting AO is
  bounded; it is not a baked albedo shadow.
- Both height and AO mip channels are made by repeated 2 by 2 byte averages.
  The sampler is linear, repeating, and has anisotropy 8.
- The full bark shader uses branch UVs, a world-position macro warp, and three
  axis projections blended by surface-normal weights. It derives a perturbed
  world normal from the sampled metric height rather than storing a tangent
  normal map. Six-layer parallax and three-step directional horizon visibility
  fade out by 12 m.
- The full oak material has a constant base pigment `srgb(96, 68, 43)`, nominal
  perceptual roughness `180/255`, and zero metallic response. Height-derived
  cavity can add up to 0.10 roughness, while a small world-space sinusoid adds
  micro variation. AO affects diffuse and specular occlusion. Dirt at root
  contact is a separate forest-soil material response and deposition field.
- There is no current moss, lichen, or general dampness output in `OakBark`.
- Aggregate wood LODs intentionally omit bark texture sampling, parallax,
  height normals, horizon visibility, and root-contact treatment. They retain
  the oak base color and roughness plus a small roughness variation. The oak
  impostor style instead uses a visibly lighter `bark_srgb` of `[116, 103, 82]`;
  that is a potential full/aggregate/impostor handoff mismatch.
- A detailed metric `ENGLISH_OAK_BARK` geometry recipe and relief evaluator
  exist only under `#[cfg(test)]`. They specify 17 mm fissure depth, 13 mm
  fissure width, 14 mm lips, 12 mm plate crowns, 0.38 m mature radius, 0.045 m
  minimum radius, a 0.72 m plate length, and branch-depth attenuation
  `[1.0, 0.62, 0.24, 0.06]`. They do not currently add runtime displacement, so
  they should be treated as test/reference logic rather than a second visible
  bark layer.
- Existing tests are strong on determinism, periodicity, output format,
  independent height/AO variation, tile-edge continuity, graph connectivity,
  short fissure edges, changing cross-section identities, tapered checks,
  sparse fibres, broad shouldered junctions, absence of plate-ownership jumps,
  and monotonic mip convergence. They do not yet establish botanical age
  behavior, projection quality at branch junctions, color calibration, overlay
  causality, or visually stable LOD handoffs.

## Evidence: what *Quercus robur* bark should express

### Age changes the bark family, not just its amplitude

The University of Göttingen's forestry botanical garden describes *Q. robur*
as initially having shiny silver-grey bark. It reports that this bark begins to
split between roughly 15 and 30 years and becomes thick, grey-brown bark with
deep longitudinal furrows over time. The same page quotes a description of
light-grey bark closely furrowed into short, narrow, vertical plates. It also
warns that bark varies enough that *Q. robur* and sessile oak, *Q. petraea*,
cannot reliably be separated by bark alone ([University of Göttingen, "Rinde
und Borke"](https://www.uni-goettingen.de/de/rinde%2Bund%2Bborke/16693.html)).
Kew independently describes common oak as having thick, rough, grooved,
dark-brown bark ([Royal Botanic Gardens, Kew, "Oak tree"](https://www.kew.org/plants/oak-tree)).

**Inference for this recipe.** A radius/age control must change pattern class:

1. young twigs and narrow branches: smooth or finely checked silver-grey skin;
2. transitional axes: sparse shallow longitudinal splits and slight grey-brown
   ridges; and
3. mature trunk and major limbs: deep branching furrows, short vertical plates,
   strong shoulders, and restrained transverse closures.

Simply multiplying the mature height field by a smaller strength leaves the
same mature topology on young limbs. It is acceptable as a temporary LOD-saving
approximation, but it is not the botanical target. The generator should expose
at least `maturity`, derived from stable tree age/radius and branch order, and
blend between a young-bark field and mature-bark field. The repository's
test-only maturity curve and branch-depth attenuation are a sound initial
contract to move into an owned runtime input if the material interface permits.

Variation between individuals should perturb plate width, plate length,
fissure branching, grey/brown balance, and overall roughness within a species
envelope. It should not switch to unrelated pine-like scales or a regular
reptile-cell pattern. A deterministic per-tree seed should remain stable across
frames and LODs.

### The dominant construction is furrow-and-plate hierarchy

Pablo Blanes' procedural bark breakdown begins by identifying the large
repeating shapes before adding microdetail. His Substance workflow scatters
primary shapes, uses Flood Fill data for per-shape gradients and height
variation, bevels transitions, and then introduces more restrained
microsurface detail. He evaluates the material on correctly UV-mapped cylinder
geometry specifically to judge pattern scale and interaction between forms
([Pablo Blanes, "Making Bark Procedural Materials in Substance 3D Designer &
Marmoset Toolbag"](https://80.lv/articles/making-bark-procedural-materials-in-substance-3d-designer-marmoset-toolbag)).
Pixar's production bark system similarly starts from artist guide curves,
interpolates a smooth directional field, integrates streamlines, and aligns
stochastic bark tiling to that field; the important transferable idea is that
bark flow is a surface direction, not an axis-independent noise texture
([Bartsch et al., "A Procedural Approach for Stylized Bark Shading," SIGGRAPH
2023](https://research.pixar.com/docs/2023.SiggraphTalks.BTG.pdf)).

**Inference for this recipe.** Keep a single causal structural graph:

1. **Macro tree form:** trunk taper, root flare, branch junctions, scars, and
   silhouette-changing bulges belong to geometry, not a repeating texture.
2. **Primary fissure streams:** mostly longitudinal, gently meandering,
   branching and terminating, 5–20 mm class width/depth on a mature trunk.
3. **Plate bodies:** short, narrow vertical masses between primary furrows;
   per-plate ramps tilt and lift a plate rather than producing a flat island.
4. **Shoulders and lips:** asymmetric rounded rises bordering deep furrows;
   they make the fissure read as separated bark layers rather than black ink.
5. **Secondary checks and closures:** shorter transverse/oblique cracks which
   divide long strips but do not form a uniform closed Voronoi mosaic.
6. **Tertiary fibres and fracture:** sparse, direction-aware breakup confined
   to plate faces and edges.
7. **Microsurface:** sub-millimetre porosity and fibres should converge into
   roughness at ordinary camera distance, not remain explicit noisy normals.

The present graph-led height field is closer to this evidence than a pure
Voronoi or layered-noise solution. Preserve its branching/termination tests.
The next material iteration should add explicit per-plate tilt/lift and a
maturity-conditioned young-bark field before increasing microdetail.

### Scan and procedural workflows are complementary

Adobe's Nikola Damjanov series presents three separate bark workflows: deriving
material from one image, baking a high-resolution photogrammetry mesh and
repairing seams, and building a fully procedural Substance Designer material
([Adobe Substance 3D, "Tree Barks with Nikola Damjanov"](https://www.adobe.com/learn/substance-3d-designer/web/tree-barks-with-nikola-damjanov)).
This is useful prior art because it treats scans as measurement/reference and
procedural graphs as controllable systems rather than mutually exclusive
religions.

**Inference for this recipe.** Retain the deterministic analytic generator,
but calibrate it against a small, licensed reference set of orthographic or
photogrammetric *Q. robur* patches that includes young, transitional, and old
bark. Extract distributions, not pixels: fissure width/depth, branch frequency,
plate width/length, plate aspect ratio, directional spectrum, and height
histogram. A scan should not be copied into the runtime or become a hidden
one-off bitmap. It should be a reproducible validation fixture with provenance,
scale, lighting removal, and license recorded.

## Inference: channel construction and causality

### Height is the structural source of truth

Build primary furrows, plate ramps, shoulders, checks, and fibres in physical
metres. Derive the visible normal and geometric cavity from that field. This
prevents a bright plate edge from claiming one shape while the normal claims
another. The current shader's screen-derivative normal from metric height is a
good fit for UV-free projection and avoids the tangent-frame ambiguity of
triplanar normal maps.

The current 32 mm declared range and approximately 26.24 mm reachable span are
plausible for strong mature surface relief, but the asymmetry must be explicit
in the recipe manifest. Otherwise an artist may tune a "32 mm" slider while
only 10.24 mm of positive lift is possible. Prefer one of:

- store signed height with declared minimum and maximum metres;
- normalize observed height to the full byte range and decode with explicit
  `min_height`/`max_height`; or
- keep the current neutral 0.5 convention but document and test the asymmetric
  clamp as intentional.

Relief bands should have budgets. Primary fissures may use centimetres,
secondary checks millimetres, and fine fibres fractions of a millimetre. Reject
any fine band whose wavelength is below two mip-zero texels at the declared
0.5 m tile scale.

### AO follows height, but is not interchangeable with color

The current broad horizon plus full-resolution local cavity construction is
causal. Retain that relationship and keep AO out of base color. A fissure can be
darker in pigment because deeper tissue is damp, weathered, or compositionally
different, but that needs its own low-frequency material rule; it should not be
the same numerical AO multiplied into albedo.

AO is a nonlinear visibility quantity. Byte-averaging it through the mip chain
is a reasonable bounded baseline, but it is not guaranteed to preserve the
visibility of unresolved deep grooves. Compare box-averaged AO against AO
recomputed from each prefiltered height mip. The preferred variant is the one
that best matches a supersampled reference cylinder at equal projected size,
not simply the one with more contrast.

The shader also computes directional horizon visibility at runtime. Verify that
prebaked AO, parallax shadow, and directional visibility do not triple-count
the same cavity. A white-furnace-like test with neutral albedo and rotating
directional light should keep energy behavior stable as the feature rotates
from lit to shadowed.

### Albedo should describe bark material and age, not illumination

Blanes explicitly advises building base color without baked lighting and uses
shared shape masks as inputs rather than copying the height map directly. The
current single oak pigment is admirably stable but underrepresents the
silver-grey young bark and grey-brown mature range.

Use a small, bounded palette or low-frequency color field controlled by:

- age/maturity;
- plate identity and exposed edge/interior state;
- restrained vertical weathering and substrate variation; and
- separately supplied damp, soil, moss, and lichen masks.

Do not add independent high-frequency brown noise. At distance, oak identity
comes from value grouping and plate/furrow rhythm; random chroma flecks turn it
into camouflage. Keep metallic at zero.

### Roughness correlates with state, not with height alone

Dry corky bark is broadly rough, but the deepest pixel is not automatically the
roughest. Plate faces, abraded shoulders, wet crevices, mineral soil, moss, and
lichen each have different microstructure. Reuse structural masks, then add
state-specific variation:

- dry plate faces: high, moderately varied roughness;
- freshly abraded or compressed lips: slightly smoother where justified;
- damp/wet bark: darker albedo with a lower and more coherent roughness/specular
  response;
- soil deposition: inherit forest-soil roughness and reflectance;
- moss: very high diffuse/fibrous roughness when dry, with wet-state response;
- crustose lichen: separate fine relief and species palette, not "green bark."

The current cavity-plus-sinusoid roughness is a useful minimal response, but its
world-space sinusoid can swim in meaning across species and LODs. A future
packed material channel should be derived from persistent structural/state
masks and prefiltered independently.

## Evidence and inference: dampness, moss, and lichen are separate systems

Research on non-vascular epiphytes identifies bark water storage as a meaningful
resource and suggests that inter-species differences in bark storage help
explain epiphyte richness ([Porada et al., "Bark Water Storage Plays Key Role
for Growth of Mediterranean Epiphytic Lichens"](https://www.frontiersin.org/journals/forests-and-global-change/articles/10.3389/ffgc.2021.668682/full)).
A Central European lichen dataset records bark pH, conductivity, water-holding
capacity, periderm-crack depth, trunk light, humidity, and diameter as distinct
host and microhabitat variables ([Łubek et al., data article on epiphytic
lichens and host trees](https://pmc.ncbi.nlm.nih.gov/articles/PMC7251647/)).

A Killarney field study found moss and lichen coverage negatively correlated,
with tree species, circumference, canopy, height, and aspect interacting. It
also reports that aspect alone was not significant and explicitly warns against
the simple rule that north/east is always the damp side; topography, trunk lean,
and canopy alter exposure
([Sales, Kerr & Gardner, "Factors influencing epiphytic moss and lichen distribution within Killarney National Park"](https://academic.oup.com/biohorizons/article/doi/10.1093/biohorizons/hzw008/2526859)).

**Inference for this recipe.** Model four separable fields:

1. **Intrinsic bark:** species, age, branch order, and persistent injury.
2. **Instantaneous wetness/dampness:** recent rain/dew, stemflow, concavity,
   canopy interception, sun/wind exposure, root splash, and drainage. It changes
   optical response but contributes no centimetre-scale bark height.
3. **Moss colonization:** long-term humidity, shade, bark water retention,
   fissure shelter, tree age, continuity, and disturbance. At close range it
   may require raised/fibrous material or small geometry rather than only
   pigment.
4. **Lichen colonization:** long-term substrate chemistry, light/exposure,
   moisture regime, stability, and species. Crustose and foliose forms need
   different relief and silhouettes.

Do not bake all four into the `OakBark` tile. `OakBark` should expose stable
substrate masks such as fissure depth, plate face, shoulder, and maturity.
Environment systems can combine those with world-space conditions. Dampness
may share the same bark geometry while changing albedo/roughness. Moss and
lichen need independent coverage, color, roughness, height, mip, and LOD rules.
Use competition or at least mutually reduced coverage rather than stacking both
at full strength. Never use a fixed "north side" mask.

The existing causal root-soil deposition is a good architectural example:
environmental material is evaluated from world position and terrain contact,
while oak relief remains its own source.

## Projection and UV prior art

A SideFX tree-mapping discussion presents two practical procedural choices:
triplanar projection, or mapping a straight trunk before deformation
([SideFX forum, "Mapping a texture in a tree"](https://www.sidefx.com/forum/post/231081/)).
Another SideFX discussion exposes the important caveat: simply blending
tangent-space normal images across three planes gives wrong orientations;
contributors recommend transforming projected normals into a common space or
using height-to-normal instead ([SideFX forum, "Mtlx triplannar normals are
incorrect on different sides"](https://www.sidefx.com/forum/topic/87614/)).
SideFX users also control triplanar scale by multiplying the input position,
which reinforces the need for explicit world/metric scale rather than a hidden
UV frequency ([SideFX forum, "Scaling control for MTLX Triplanar
Projection"](https://www.sidefx.com/forum/topic/81993/)).

**Inference for this recipe.** The repository is already using the safer
height-to-world-normal path, but ordinary world-axis triplanar projection is
still a poor primary coordinate system for strongly longitudinal oak. It can
rotate the fissure direction at axis changes and show doubled or blurred
patterns in blend zones. Use this hierarchy:

- branch-local cylindrical or sweep/growth coordinates for the directional
  primary fissure and plate pattern;
- a continuous branch frame or guide field through junctions where practical;
- triplanar/world projection only to conceal junction seams and add
  low-amplitude
  nondirectional macro breakup; and
- derivative-based world normals from the final blended metric height.

The current shader's 92% alignment to branch coordinates is already a hybrid
in this direction. Test it on vertical trunks, horizontal limbs, diagonals,
root flares, and Y-junctions. A successful blend preserves longitudinal flow,
physical feature width, and phase continuity; merely hiding a UV seam is not
enough.

Keep scale metric. A 50 mm plate or 5 mm check should remain that size on a
0.1 m branch and a 1 m trunk. What changes with radius is maturity/pattern
family and the number of features around the circumference, not arbitrary UV
stretch. If a narrow branch cannot represent mature plates without overlap,
select the young-bark field.

## Mips, distance behavior, and tree LODs

Blanes' cylinder evaluation demonstrates why a flat tile preview is
insufficient: the same pattern must be judged at its intended curvature and
scale. The GDC 2015 *Far Cry 4* vegetation presentation describes three trunk
and leaf LODs before a tree becomes an impostor, and its session overview
emphasizes a continuous progression from close geometry to distant vegetation
representations ([Stephen McAuley, "Rendering the World of Far Cry
4"](https://www.gdcvault.com/play/1022234/Rendering-the-World-of-Far),
[slide deck](https://media.gdcvault.com/gdc2015/presentations/McAuley_Stephen_Rendering_the_World.pdf)).
The transferable requirement is not Ubisoft's exact tier count; it is that the
bark's average value, silhouette contribution, and frequency energy converge
before the representation changes.

**Inference for this recipe.** Define three material-frequency regimes even if
mesh LOD names change:

- **near:** full height-derived normal, bounded parallax, directional response,
  and environmental overlays;
- **middle:** no parallax, but prefiltered height normal/AO and stable mean
  color/roughness; and
- **far/impostor:** no individual furrows, but the same aggregate trunk value,
  broad longitudinal contrast, and compatible roughness/lighting response.

The current full shader fades parallax and directional horizon visibility by
12 m, while aggregate wood removes the entire texture pipeline. That is a good
cost boundary only if the lower-cost tier preserves the visual integral. Match
the full bark's mean linear albedo under neutral light, perceptual roughness,
and broad normal variance at the transition. Reconcile the lighter impostor
`[116, 103, 82]` with full/aggregate `[96, 68, 43]` or document a compensated
impostor bake; otherwise trunks will brighten as trees recede.

Box filtering is appropriate for scalar mean height, but parallax is not linear
in mean height and AO is not linear in visibility. Evaluate alternatives:

1. retain box-filtered height and AO as the inexpensive baseline;
2. generate per-mip AO from each filtered height field;
3. store height min/max or variance for conservative parallax and normal fade;
4. explicitly reduce relief strength with projected texel footprint before
   switching material tier.

Prefer the least costly variant that removes shimmer and LOD popping in motion.
Do not preserve small fissures by artificially sharpening every mip: unresolved
features should converge into stable aggregate roughness/value instead.

## Recommended generator structure

The following is a source-backed implementation direction, not a claim that the
repository already has it.

1. Create a deterministic branch-local directional field from the growth axis
   and any junction guide data.
2. Generate a mature primary fissure graph with mostly longitudinal edges,
   limited branch angles, finite segments, and explicit junction/termination
   probabilities.
3. Form plate regions from the graph, but use smooth per-plate ramps and lifts;
   never expose nearest-cell ownership discontinuities.
4. Build shouldered fissure profiles from metric width/depth, with asymmetric
   lips and rounded valleys.
5. Add sparse transverse checks whose start/end are constrained by the plate
   graph; reject closed regular cell outlines and horizontal row bands.
6. Add restrained plate-face fibres and fracture only after the primary form
   passes cylinder tests.
7. Generate a separate young-bark field and blend topology by maturity, radius,
   and branch order.
8. Derive metric height, world normal, and AO from the shared structure. Build
   albedo and roughness from persistent material masks, not copied lighting.
9. Export stable substrate masks for external damp/moss/lichen owners.
10. Generate channel-aware mips and lower-tier aggregate descriptors from the
    same source statistics.

Do not replace the current graph wholesale merely to follow a Substance node
recipe. Its strongest parts—shared metric plate field, short explicit fissure
graph, finite checks, and ownership-jump tests—already embody the practitioner
principles. Improve the missing age, surface-direction, material, and LOD
dimensions around that core.

## Acceptance and testing plan

### Deterministic analytic tests

Retain all existing determinism, periodic seam, ownership-continuity, graph,
fissure, check, fibre, AO, and mip-span tests. Add:

- **physical scale:** report and bound distributions in metres for primary
  fissure width/depth, plate width/length, shoulder width, check width/length,
  and fibre wavelength;
- **height encoding:** assert declared minimum, neutral point, maximum, and
  decoded metric span; reject silent clamp loss;
- **anisotropy:** Fourier or gradient-energy ratio should show stronger
  longitudinal structure in mature bark without collapsing into uninterrupted
  stripes;
- **topology:** bound closed-cell count, graph degree distribution, terminal
  count, junction count, and uninterrupted-furrow length;
- **age/radius:** narrow/high-order samples must select smooth young structure;
  mature trunk samples must gain fissure depth, branching, and plate contrast
  monotonically without a hard threshold;
- **channel causality:** normals and AO must reproduce the same height field;
  albedo must remain unchanged when only light direction changes; moss/lichen
  masks must not exist without their environmental owner;
- **periodicity including derivatives:** test value and first-derivative error
  on both tile axes, not value alone;
- **mip semantics:** preserve mean height, bound normal variance and AO drift,
  and converge unresolved relief without span re-expansion; and
- **LOD descriptors:** full, aggregate, and impostor mean linear bark color and
  roughness must remain within frozen tolerances.

### Static visual fixtures

Use production shading and fixed exposure. Capture the same seed on:

1. a straight 0.08 m radius young branch;
2. a 0.2 m transitional limb;
3. a 0.5–0.8 m mature vertical trunk;
4. a horizontal limb;
5. a diagonal branch;
6. a Y-junction and root flare;
7. dry, damp, mossed, and lichened variants with overlays isolated; and
8. neutral grey cylinder references at exact physical scale.

For every subject capture diffuse frontal light, hard grazing light, wet/dry
specular comparison, and a silhouette/grazing camera. Include a metric ruler or
known cylinder circumference in the lab fixture. The reviewer should be able to
identify young versus mature bark without seeing labels.

Reject:

- uniform Voronoi cells or alligator skin;
- unbroken vertical fluting;
- plate ownership seams;
- black painted cracks with no metric valley;
- inflated shoulders at triplanar blends;
- mature fissures miniaturized onto twigs;
- identical moss and lichen coverage;
- baked-light albedo;
- root soil appearing away from contact; and
- a trunk value jump at full/aggregate/impostor transitions.

### Temporal and distance fixtures

Run a slow orbit, forward/back dolly, and wind-enabled branch sequence through
every material and mesh LOD boundary. Freeze camera path, seed, resolution,
exposure, lighting, and tree state. Compare frames and a difference video for:

- mip shimmer and moire in longitudinal furrows;
- parallax swimming or reversal at grazing angles;
- triplanar blend motion at branch junctions;
- normal/AO sparkling as features become subpixel;
- abrupt relief loss at 12 m;
- color or roughness popping into aggregate wood;
- impostor trunk brightening; and
- unstable damp/moss/lichen masks under wind or LOD change.

The acceptance reviewer should see baseline and candidate in randomized order
and should not receive the implementer's intended conclusion. Any obscured,
misframed, stale, or non-production capture is `UNASSESSABLE`, not a pass.

### Performance tests

Measure full, middle, aggregate, and impostor tiers separately. Record texture
generation time, generated bytes including mips, upload volume, shader/pipeline
compilation, bark draw calls, visible trunk pixels, texture samples, steady GPU
median/p95/p99, and temporal hitching. Test dense mature-oak scenes as well as a
single lab tree. Do not benchmark while other compilation or capture workloads
are active.

Compare equal tree geometry, camera, lighting, and image quality. The high-cost
reference may use full parallax and recomputed per-mip AO; the shipping choice
should be the smallest Pareto-efficient variant that passes the frozen visual
gates. Results from cited offline or AAA pipelines are conceptual prior art,
not comparable performance ceilings for this Bevy/WGSL implementation.

## Source classes, limits, and applicability

- **Botanical authority:** University of Göttingen and Kew establish the
  species/age morphology. Their prose does not provide a complete metric height
  distribution; dimensions still require licensed measured reference.
- **Peer-reviewed ecology:** the Frontiers, PMC, and Oxford studies establish
  that water storage, bark traits, tree size, canopy, exposure, and competition
  matter to epiphytes. Mediterranean and Irish field results are not a direct
  numerical preset for 1544 Germany; they justify causal variables and reject a
  universal compass-side shortcut.
- **Practitioner breakdown:** Blanes and Adobe document repeatable procedural,
  scan, baking, seam-repair, and cylinder-validation workflows. They do not
  report game-runtime costs or *Q. robur*-specific measurements.
- **Conference production:** Pixar supports direction-field-aligned bark
  synthesis; its stylized film shading, Ptex, and offline pipeline are not a
  drop-in runtime method. GDC's *Far Cry 4* material supports deliberate
  vegetation LOD progression, but its hardware, content, and engine are not
  comparable performance data.
- **Houdini forum evidence:** SideFX discussions are practitioner observations,
  useful for mapping options and normal-space failure modes. They are not
  controlled benchmarks. The repository should validate every projection
  recommendation in its own WGSL fixtures.

## Ordered priorities

1. Preserve the existing graph and prove its physical feature distributions on
   a metric cylinder.
2. Add or expose age/radius/branch-order control with a genuinely smooth young
   bark family.
3. Make branch-local directional coordinates primary and validate junctions;
   keep height-derived world normals.
4. Calibrate albedo and roughness for young/transitional/mature dry oak without
   baked illumination.
5. Reconcile near, aggregate, and impostor mean color/roughness and capture the
   12 m relief fade in motion.
6. Compare box AO mips with AO recomputed from filtered height and retain the
   cheaper stable result.
7. Define external, independently testable damp, moss, and lichen interfaces;
   do not fold them into the intrinsic bark tile.
8. Only after these pass, add restrained close-range fibres or additional
   microdetail.

This order protects the species read and distance stability before spending
samples on detail that disappears in ordinary play.
