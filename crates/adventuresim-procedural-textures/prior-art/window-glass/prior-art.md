# Early-modern window-glass prior art

## Scope and historical contract

This report concerns exactly the `WindowGlass` procedural optical material. Its
job is to describe the glass inside a pane: transmitted tint, optical surface
normal, thickness variation, and microsurface roughness. Lead cames, wooden or
iron frames, mullions, bars, putty, latches, hinges, broken edges, and the shape
and operability of a window remain geometry or object-space masks.

The setting is Germany in 1544. “Old glass” must not be treated as a single
modern float-glass sheet with decorative wobble. Hand-made window glass in the
relevant northern-European craft family was made by crown or cylinder/broad
(`muff`) methods. Both processes predate the setting, and trade complicates any
simple national label. A recent fenestration-history project identifies French
crown and Lorraine/Rhine-area cylinder production as dominant market centres
until the last quarter of the sixteenth century
([FENESTRA final report](https://www.belspo.be/belspo/brain-be/projects/FinalReports/FENESTRA_FinRep.pdf)).
Research on fifteenth- and sixteenth-century Low Countries glass similarly
connects the region to those production centres and finds that technique,
composition, thickness, pane size, and leadwork all affect transmitted daylight
([Heritage Science: interaction between daylight and fifteenth- and
sixteenth-century glass windows](https://pmc.ncbi.nlm.nih.gov/articles/PMC8556355/)).

**Evidence boundary.** These sources establish plausible manufacturing families
and optical consequences, not a census of every German urban window. Glazing
availability, pane pattern, purity, decoration, and repair history should vary
by building wealth, date, room, and civic or religious importance. The material
recipe should support those choices rather than silently declare one universal
window type.

**Inference for the recipe.** Implement crown and cylinder as distinct
deterministic variants or parameterized process states. A pane should receive
one coherent process signature. Do not superimpose crown rings, parallel
cylinder bubbles, generic waves, and a bull's-eye on every pane.

## Repository facts and constraints

The following are current repository observations, not external claims.

- `window_glass.rs` owns a deterministic 512 by 512 repeatable recipe. One
  repeat is declared as 2.4 metres square, about 4.69 mm per base texel.
- It declares nominal thickness as 3.2 mm and thickness variation as 1.2 mm.
  The generated R channel is normalized but the module does not provide a
  physical decode formula that maps a byte to metres.
- It emits sRGB RGB transmittance with opaque alpha, OpenGL tangent-space
  optical normal, and packed RG thickness/roughness. All outputs have ten mips
  and repeat sampling.
- The material contract specifies IOR 1.52, specular transmission 0.96, no
  diffuse transmission, a green-biased attenuation color, 0.42 m attenuation
  distance, perceptual roughness 0.13, double-sided rendering, and fallback
  alpha 0.24.
- The runtime Bevy `StandardMaterial` uses only the 3.2 mm scalar thickness. It
  binds packed G as roughness but cannot consume packed R as per-texel
  thickness.
  Thus thickness variation currently exists only as future-shader and review
  data.
- `fallback_alpha` is documented as a fallback for a renderer unable to transmit
  scene color, not as an opacity map. The transmittance texture's alpha is 255.
- The optical field uses several global sinusoidal waves, four localized
  striation patches, and six sparse stretched bubble lenses. It does not choose
  a crown or cylinder process per pane, know pane boundaries, or orient features
  from a source disc/cylinder.
- Tests establish determinism, analytic optical-height seam continuity,
  restrained greenish transmitted tint, varied but bounded normalized thickness
  and roughness, and mip completeness. They do not validate physical thickness
  decoding, process morphology, Fresnel/refraction behavior, transparent
  sorting,
  transmitted shadows, runtime scene-color transmission, normal mip length, or
  appearance through building LODs.
- The existing evidence exporter explicitly marks actual `StandardMaterial`
  transmission as unassessable: its CPU fixture is only a distortion proxy. A
  frozen GPU capture with opaque high-contrast geometry behind the pane and a
  no-glass reference is still required.
- Candidate history usefully rejects a universal high-frequency corrugation,
  conspicuous repeated landmarks, and an ambiguous runtime thickness claim.
- The shared image mip helpers average encoded bytes. They do not filter sRGB
  transmission in linear light, decode and renormalize optical normals, or
  account for unresolved slope variance in roughness.
- No consumers outside the procedural material module generate per-pane process
  coordinates or thickness masks today. The eventual window assembler must own
  that relationship.

## Period craft: two process families

### Crown glass is radial because it was spun

**Evidence.** Historic England describes crown glass as a blown bubble
transferred to a pontil and spun into a disc. It was generally thinner and had a
better surface than broad glass because it did not need flattening. Its central
pontil area—the bull's-eye—was thick and usually discarded; small quarries were
cut from the remaining disc. Subtle curvature, wave, concentric markings, and
concentrically arranged bubbles could remain
([Historic England: Archaeological Evidence for Glassworking](https://historicengland.org.uk/images-books/publications/glassworkingguidelines/heag259-archaeological-evidence-for-glassworking/),
[Historic England: Origins and Use of Medieval Glazing](https://historicengland.org.uk/whats-new/research/back-issues/the-origins-and-use-of-medieval-glazing-in-england/)).
An archaeological study likewise describes a fire-polished surface and
concentric lines of small bubbles resulting from spinning
([Scottish Archaeological Journal: Cathcart Castle window glass](https://doi.org/10.3366/saj.2016.0073)).

**Inference for the recipe.** Crown glass needs source-disc coordinates before
the pane is cut. Generate a disc-space field with:

1. broad radial thickness falloff and low-amplitude dish/curvature;
2. sparse concentric or tangential bubble alignments;
3. faint, irregular circumferential working lines;
4. a smooth fire-polished microsurface; and
5. optional thick centre/rim states only when the asset intentionally uses
   those less desirable portions.

Then cut pane-shaped windows from deterministic positions on that virtual disc.
Each quarry should inherit an arc fragment, not re-centre a perfect bull's-eye
inside itself. Neighboring panes may come from different source discs unless an
asset deliberately models a batch. A repeated radial center in every diamond
would look like bottle bottoms and falsely imply the expensive/waste centre was
universally installed.

### Cylinder or broad glass is directional because it was blown and flattened

**Evidence.** The V&A describes the broad-sheet/muff process: a blown bubble was
worked into a cylinder, cut lengthwise, reheated, and pressed flat
([V&A: Stained glass, an introduction](https://www.vam.ac.uk/articles/stained-glass-an-introduction)).
Historic England notes that the flattening surface could leave one side slightly
rough and that broad glass had subtle curve or wave. FENESTRA reports that
elongated bubbles indicate cylinder glass while circularly arranged bubbles
indicate crown glass. Heritage Science observations likewise associate straight,
parallel elongated bubbles with cylinder forming
([Heritage Science: Prestige markers in fifteenth-century stained glass](https://www.nature.com/articles/s40494-022-00698-2)).

**Inference for the recipe.** Cylinder glass needs sheet coordinates with an
explicit former-cylinder axis and cut seam. Generate:

- broad unflattened curvature and low-frequency thickness drift;
- sparse bubbles elongated broadly parallel to the working direction;
- intermittent draw/cord striations with shared local orientation;
- a small, asymmetric roughness difference between fire surface and flattened
  surface if the renderer/mesh preserves front and back; and
- quiet regions, because archaeological examples can be visually clear and not
  every pane displays conspicuous inclusions.

Do not tile uninterrupted sinusoidal corrugation across the whole sheet. Do not
randomly rotate every bubble independently. Directional evidence should be
coherent within a source sheet, while phase, severity, and crop change between
deterministic batches.

### Process cannot be inferred from thickness alone

**Evidence.** Historic sources describe crown as often thinner and smoother,
but archaeological analysis warns of overlapping thickness distributions.
Fifteenth-century studied glass has been reported around 2.20-2.37 mm on average
with composition/process-dependent variation, while broader medieval datasets
often fall around 1.5-3 mm
([Heritage Science: Prestige markers](https://www.nature.com/articles/s40494-022-00698-2),
[Heritage Science: Reims Cathedral glass thickness](https://pmc.ncbi.nlm.nih.gov/articles/PMC6397263/)).
The Low Countries daylight study uses fixed 3 mm and 2 mm cases to isolate
composition and thickness effects and finds that greener later glass can still
transmit more light when thinner.

**Inference for the recipe.** The current 3.2 mm nominal thickness is near but
slightly above the commonly cited 1.5-3 mm evidence band. It may remain
plausible for a robust or uneven pane, but should not be the unexamined
universal value. Define process- and grade-specific thickness distributions in
metres, and store an explicit normalized-channel decode such as nominal plus
signed variation. Thickness is a continuous property; process identity comes
from joint evidence in bubble direction, surface character, and source geometry.

## Waviness, bubbles, cords, and inclusions

### Waviness should distort the view without turning the pane into water

**Evidence.** Historic England calls the surviving imperfections subtle: a
slight curve or wave distorts the view through both broad and crown glass. Its
traditional-windows webinar emphasizes distinctive uneven reflection and light,
not opacity or a dense embossed pattern
([Historic England: Traditional Windows webinar](https://historicengland.org.uk/education/training-skills/training/webinars/recordings/webinar-on-traditional-windows-care-repair-and-improving-energy-efficiency/)).

**Inference for the recipe.** Build optical height from a few broad, low-slope
process modes and localized cords. Validate angular deflection through a real
pane thickness and IOR rather than judging a grayscale normal preview. A facade
seen through the window should bend gently and continuously; it should not swim,
double, blur uniformly, or behave like a rippling pond.

Macroscopic bow belongs partly to pane geometry. If a close pane visibly bows
from its cames or frame, give it a few mesh subdivisions and small deterministic
out-of-plane displacement. Reserve the texture normal for smaller-scale optical
surface variation. Otherwise silhouette, reflection, and refraction will
disagree at grazing angles.

### Practitioner precedent: pane-specific old glass in a real-time reconstruction

**Evidence.** Van der Heijden et al.'s peer-reviewed technical breakdown of the
virtual reconstruction of Rembrandt's circa-1600 birthplace documents a
Blender/Substance/Unreal Engine 4 pipeline built from archival, archaeological,
and surviving-building evidence. For its old window glass, the team smoothed a
procedural noise field and broke it up separately for each glass panel, stored
the resulting unevenness in the normal map so the real-time renderer could
alter reflection and light breaking, and varied dirt using proximity to the
lead frame. Glass, lead/frame, shutters, and metalwork remained separate
components; code synchronized their transforms when a window or shutter
opened. The authors explicitly describe the result as a real-time interactive
Unreal scene rather than an offline material study
([Van der Heijden et al., "Virtual Reconstruction of the Birthplace of
Rembrandt van Rijn"](https://isprs-archives.copernicus.org/articles/XLII-2-W15/397/2019/isprs-archives-XLII-2-W15-397-2019.pdf)).

A narrower practitioner tutorial by technical VFX artist Jen S Abbott builds a
two-sided translucent Unreal master material with parameterized specular,
roughness, opacity, index-of-refraction refraction, and surface-forward shading
for local-light and image-based reflections. Abbott explicitly exposes
refraction as the control that changes whether the window reads smoother or
wavier and enables the engine's ray-traced translucency path for the example
([Jen S Abbott, "Making a glass material in Unreal Engine
5.3.2"](https://jsabbott.artstation.com/blog/WBBGz/making-a-glass-material-in-unreal-engine-getting-started-in-unreal-engine-5-3-2)).

**Inference for this recipe.** These are credible implementation precedents for
four bounded decisions already proposed here: variation must be pane-aware;
small old-glass unevenness can drive the optical normal; edge dirt belongs to
installation geometry/proximity rather than the repeating glass substrate; and
all optical and structural parts of an opening must share the operable
transform. Abbott's parameterized master/instance split also supports exposing
quality/process controls without duplicating shaders.

Neither source establishes the morphology or metric amplitude of sixteenth-
century crown versus cylinder glass. The reconstruction paper reports generic
smoothed noise rather than source-disc/source-cylinder coordinates, and the
Abbott tutorial is generic modern real-time glass with no historical craft
claim or measured distortion. Use them as adjacent technical-art evidence for
runtime ownership and controls, not as authority for bubble direction,
thickness, process identity, IOR, or acceptable waviness. Those targets remain
grounded in the historical and archaeological sources above and require the
project's own metric/capture validation.

### Bubbles and cords are sparse process evidence

**Evidence.** Crown manufacture can organize small bubbles on circular tracks;
cylinder manufacture can elongate them into parallel lines. Archaeological
descriptions use bubbles, intrusive impurities, tint, thickness, profile, and
grozing/lead stains together to identify fragments. Some well-preserved pieces
in the Heritage Science study are clear and nearly free of visible defects.

**Inference for the recipe.** Treat bubbles as a sparse distribution with a
long quiet tail, not mandatory “seeded glass.” A visible bubble should affect:

- optical normal on both interfaces or an equivalent lens approximation;
- local thickness/optical path if it is an entrained inclusion;
- transmitted distortion more strongly than albedo;
- only a restrained roughness change unless weathered at the surface; and
- the correct crown-tangential or cylinder-parallel orientation distribution.

At current scale, the six hard-coded bubbles have deterministic placement but
will repeat across every 2.4 m tile. Prefer a per-source-sheet seed and a
stochastic count with bounded density, dimensions recorded in millimetres, and
a test that allows genuinely clear crops. Prevent obvious landmark repetition
between adjacent panes.

“Striation” should also be divided into internal cord, surface flattening mark,
and broad sheet wave. These have different optical and roughness consequences;
one generic height stripe should not drive them all.

## Color, transmission, thickness, roughness, and normal

### Transmission color is not opaque base color

**Evidence.** Research on fifteenth- and sixteenth-century clear glazing finds
that iron impurities in raw materials imparted non-white tint and that both
composition and thickness influenced transmitted daylight. Small panes and lead
cames reduced the whole window's light transmission relative to a modern
window. The glass itself could nevertheless remain transparent
([Heritage Science: daylight and early-modern windows](https://pmc.ncbi.nlm.nih.gov/articles/PMC8556355/)).

**Inference for the recipe.** Preserve the existing distinction between RGB
transmittance and fallback alpha. A green, blue-green, straw, or grey bias
should be modeled as absorption over physical distance, not painted as an opaque
blue rectangle. A 2 mm and 4 mm area of the same batch should differ because of
optical path length. The frame/came area blocks light through geometry; the
glass texture should not pre-darken itself to compensate for missing structure.

Provide a few restrained composition/grade families rather than arbitrary hue
noise. Variation within one source sheet should be smaller than variation across
batches. Stained, flashed, grisaille-painted, or silver-stained glass should use
separate decorative layers and historical asset decisions, not the default
plain `WindowGlass` recipe.

### Use a dielectric interface with Fresnel response

**Evidence.** GDC's `Advanced Material Rendering` session treats glass as a
special transparent material whose reflection, refraction, performance, and
deferred-renderer constraints must be solved together
([GDC Vault: Advanced Material Rendering](https://www.gdcvault.com/play/1013725/Advanced-Material)).
SideFX practitioners repeatedly encounter the same coupling: a window needs
clear transmission and reflection, but naïve IOR/refraction settings can
over-distort a nearly planar thin pane, while transparent shadow handling is a
separate problem
([SideFX forum: Render a glass window pane](https://www.sidefx.com/forum/topic/91963/),
[SideFX forum: Glass shader problems](https://www.sidefx.com/forum/topic/41058/)).

**Inference for the renderer.** Keep IOR around an evidence-informed ordinary
glass value, but make the thin-pane approximation explicit. Reflection should
increase toward grazing angles while face-on transmission remains high. Optical
normal perturbs both the reflected environment and the background/refraction;
an alpha-blended blue panel with a static highlight is not enough.

The current IOR 1.52 is a reasonable generic glass contract, but the visible
distortion must be calibrated from interface geometry, thickness, and optical
normal—not by reducing IOR toward air until an overpowered normal map becomes
tolerable. If the engine uses a thin-walled approximation, document whether it
models two interfaces, one effective offset, or only screen-space background
distortion.

### Channel relationships must remain physically legible

**Evidence.** Historical thickness variation changes absorption and optical
path. Surface waviness changes reflected/refracted direction. Surface roughness
broadens reflection/transmission lobes. These are related through manufacture
but are not interchangeable measurements.

**Inference for the recipe.** Maintain named intermediate fields:

1. source-sheet shape and process coordinates;
2. physical front/back surface displacement;
3. physical thickness in metres;
4. entrained bubbles/cords;
5. surface roughness/flattening contact;
6. glass composition and absorption; and
7. optional weathering, dirt, condensation, paint, or repair layers.

Derive maps from those fields, but do not copy one channel into another.
Broad thickness drift can affect transmission without producing a matching
surface normal. A smooth bowed pane can strongly distort reflection while
remaining low roughness. Fine corrosion or grime can raise roughness and reduce
transmission without changing bulk thickness. A bubble lens can perturb optical
normal while leaving the outer microsurface polished.

The current `thickness` byte should gain a documented metre decode and a runtime
consumer before it is claimed as visible. Until then, review reports must say
that only scalar thickness is rendered.

## Leaded panes and window geometry

### Cames, putty, and support are structural

**Evidence.** The V&A describes individual cut pieces fitted into H-shaped lead
cames, soldered at intersections, puttied for weatherproofing, and attached to
an iron armature. Cames are both structural and part of the visual design. It
also notes that sixteenth-century diamond cutters enabled cleaner glass edges,
although grozing remained part of earlier fitting practice
([V&A: Stained glass, an introduction](https://www.vam.ac.uk/articles/stained-glass-an-introduction)).

**Inference for the window assembler.** Model lead cames with thickness, rounded
or weathered profile, solder nodes, and depth relative to the glass. Place each
glass piece in the came channel with a slight deterministic seating offset and
possibly a small bow. Do not bake a universal diamond grid into the glass tile.

The assembler should own:

- pane polygon and cut pattern: quarry, lozenge, square/rectangular, roundel, or
  intentionally reused disc centre;
- came network and solder topology;
- frame/mullion/armature, putty, bars, and opaque occlusion;
- pane batch/process identity and source-sheet crop;
- front/back orientation and installed thickness;
- operable sash/casement transform, hinge, latch, and collision; and
- broken, missing, patched, painted, or repaired pane states.

The texture should own only what remains continuous inside the cut glass. The
glass edge is important at close range, but it is generated from pane geometry:
real thickness, an irregular/grozed or cleaner cut profile as appropriate, and
edge tint from the longer optical path. It is not a dark outline in albedo.

### Operable windows require material and geometry to move together

**Inference for the project.** A casement's panes, cames, frame, interaction
target, collider, and optical geometry must share one transform. Opening it
changes reflection, background, shadow, and ordering. A static facade decal
cannot become an interactive window merely by hiding collision.

Use the same glass recipe on fixed and opening panes, but let the asset state
control dirt/water orientation, interior/exterior face, hinge-side wear, and
visibility. Broken or open windows should reveal the actual interior rather
than fading an opaque blue material.

## Real-time transparency and refraction constraints

### Transparent sorting is an architectural problem, not only a shader problem

**Evidence.** GDC presentations on adaptive order-independent transparency and
Creative Assembly's move away from sorted particles demonstrate that overlapping
transparent geometry creates ordering and performance problems in real-time
engines
([GDC Vault: Adaptive Order Independent Transparency](https://www.gdcvault.com/play/1014547/Adaptive-Order-Independent-Transparency-A),
[GDC Vault: Instancing and Order Independent Transparency in Total War](https://www.gdcvault.com/play/1026177/Instancing-and-Order-Independent-Transparency)).
SideFX forum reports likewise show that viewport transparency, refracted
shadows, and nested transparent surfaces can fail independently.

**Inference for this project.** Establish an explicit renderer ladder:

1. **Near/hero:** specular transmission, Fresnel reflection, optical-normal
   refraction/distortion, physical or effective thickness, and correct ordering.
2. **Mid:** thin-walled transmission with filtered optical normal, stable
   reflection, and perhaps no expensive secondary transparent layers.
3. **Far:** an opaque or masked facade representation that bakes average pane
   value/reflection and the came pattern, without trying to sort hundreds of
   tiny transparent quarries.

Avoid overlapping coplanar glass shells. A single plane with a declared
thin-walled shader is preferable to two nearly coincident planes when true solid
refraction is unavailable. If solid refraction is used, mesh front and back as a
coherent thin volume with correct normals and no gaps.

Screen-space refraction can only sample already rendered scene color, so it may
miss off-screen objects, other transparent panes, and geometry rendered later.
Treat those as documented limitations. It must never sample the window itself
recursively or bend HUD elements. A fallback should preserve reflections and a
subtle tint rather than turn the pane opaque.

### Shadows and daylight need a separate acceptance contract

**Evidence.** SideFX practitioners note the failure mode in which nominally
transparent window glass casts an opaque shadow, or conversely removing the
shadow makes the pane appear emissive. Historical research emphasizes that the
lead/pane assembly—not just glass absorption—shapes interior daylight.

**Inference for the renderer.** Opaque cames, bars, mullions, and frames should
cast crisp-to-soft structural shadows. Plain clear glass should transmit most
direct light, modulated by absorption and any supported rough transmission; it
should not make interiors black. If the renderer cannot refract or tint shadow
rays accurately, use a documented cheap transmitted-light approximation, not
the material's fallback alpha as opaque shadow density.

## Metric UVs, process coordinates, and variation

### One 2.4 m repeat is not a pane model

**Evidence.** Crown discs and cylinder sheets have process coordinates larger
than the quarries cut from them. Leaded windows assemble many individually cut
pieces. The optical signature therefore depends on where a pane came from in a
source disc/sheet, not merely on its final local 0-1 UV.

**Inference for the asset pipeline.** Give each pane both:

- **metric local UVs**, with a documented metres-per-UV scale so bubble, cord,
  and wave dimensions remain physical; and
- **source-process coordinates**, identifying crown-disc radius/angle or
  cylinder-sheet axis/crop plus deterministic batch seed.

Do not map every small pane over the full 2.4 m texture, which would shrink
metre-scale waves and all six bubbles into each quarry. Do not sample one shared
building-wide 2.4 m world projection either, which would make optical features
continue through cames as if the panes were never cut. Crop a process field per
pane at metric scale and decorrelate batches deterministically.

For crown panes, retain radial direction after cutting. For cylinder panes,
retain the sheet working axis. Pane rotation in a lead pattern rotates the glass
crop physically; it should not silently rotate the process field back to world
up unless installation rules say so.

At the declared 4.69 mm per texel, millimetre bubbles and fine cords are below
base resolution. Either tighten the physical repeat, separate macro and micro
maps, or accept that the recipe represents only centimetre-to-metre optical
variation. Record feature dimensions and reject claims that a sub-texel feature
is represented merely because a noise function contains it.

## Mips, temporal stability, and LOD

### Optical fields need semantic filtering

**Evidence.** Normal vectors, sRGB color, scalar thickness, and roughness are
different quantities; averaging their encoded bytes does not preserve their
meaning. Transparent details are especially sensitive because a small normal
change moves the background sample rather than merely changing local shading.

**Inference for this repository.** Generate semantic mip chains:

- decode sRGB transmittance to linear, filter optical transmission/absorption,
  then re-encode;
- decode optical normals, average vectors, and renormalize;
- carry unresolved normal variance into a broader effective roughness or reduced
  distortion amplitude;
- filter physical thickness in metres, not arbitrary normalized bytes;
- preserve sparse bubble/cord energy without allowing a one-pixel refraction
  offset to flicker between frames; and
- validate the terminal mip against the area-average transmission and average
  surface orientation of the source field.

For refraction, choose mip level from projected pane/feature footprint and apply
continuous LOD transitions. Distant panes should become optically calmer, not
alias into sparkling or swimming facade pixels. The mean glass should retain a
subtle angle-dependent reflection so it does not disappear completely.

### Building LODs should simplify the assembly coherently

**Inference for the building system.** Use distance, projected pane size, and
transparent-overdraw budget—not “playable area”—to select representation:

- LOD0: pane geometry, cames/frame geometry, interactive transform, hero glass;
- LOD1: simplified pane/came geometry, shared thin-glass material, reduced or
  omitted per-pane refraction;
- LOD2: opaque facade atlas with baked came/pane appearance, stable roughness
  and reflection cue, no transparent sorting; and
- cull/terminal facade: average window value consistent with exposure and
  interior lighting policy.

Do not let glass vanish before its dark came/frame pattern or turn into
saturated blue holes. When switching from transmission to a baked facade, match
average luminance and reflection under a controlled capture so the window does
not pop. Operability and collision remain near-detail concerns, but visual
placement is the same unified building placement system at all distances.

## Recommended procedural representation

The minimum useful implementation split is:

1. **Process family:** crown or cylinder/broad; no universal hybrid.
2. **Source geometry:** virtual disc radius/angle or cylinder-sheet axis/crop.
3. **Batch composition:** restrained absorption/tint and quality grade shared
   across related panes.
4. **Physical surfaces:** broad front/back shape, thickness in metres, and
   process-specific asymmetry.
5. **Sparse inclusions:** bubbles and cords with process-aligned shape and a
   clear-pane probability.
6. **Installation geometry:** pane polygon, cames, putty, frame, bars, edge,
   seating, bow, and operable transform supplied by the consumer.
7. **Condition layers:** clean, dusty, soot/grime, condensation,
   painted/stained,
   weathered, cracked, patched, or missing—each object- and side-aware.
8. **Renderer contract:** hero transmission/refraction, thin fallback, and far
   opaque representation with explicit limitations.
9. **Semantic outputs/mips:** linear optical quantities and normalized vectors,
   not generic byte pyramids.

The current analytic candidate already makes good foundational decisions: it
keeps transmittance distinct from alpha, avoids dense bubbles, localizes
striations, and documents the scalar-thickness limitation. The next iteration
should add process identity, source coordinates, a metre decode for thickness,
pane-aware variation, semantic mips, and an actual runtime GPU fixture before
increasing surface detail.

## Deterministic tests and visual acceptance

### Automated tests

Retain generation determinism, analytic seam continuity, tint bounds, and mip
completeness. Add tests for:

- exact metadata: texture size, tile metres, texel metres, process coordinate
  convention, IOR, physical thickness decode, tangent convention, channel
  packing, and fallback behavior;
- distinct crown/cylinder generators with deterministic batch and pane seeds;
- crown angular/tangential feature statistics and cylinder parallel elongation
  statistics, rejecting a universal isotropic hybrid;
- physical bubble and cord size/density bands in millimetres, including a
  nonzero probability of visually clear panes;
- absence of dominant high-frequency full-tile spectral stripes and repeated
  landmark synchronization across neighboring panes;
- source-crop continuity inside a pane and intentional
  discontinuity/decorrelation
  across came-separated pieces;
- metre-scale thickness bounds and correct normalized encode/decode round trips;
- channel causality: thickness changes attenuation; optical slope changes
  normal;
  roughness does not equal height; tint contains no baked reflection or shadow;
- decoded normal unit length and bounded angular error at every mip;
- linear-light transmission mip reference values and terminal average;
- temporal distortion stability under subpixel camera motion and every mip/LOD;
- transparent sorting scenes with two windows, open casements, foliage, and a
  character behind multiple panes;
- shadow contract: glass does not cast an opaque slab shadow while came/frame
  geometry still casts structure;
- no-glass, hero-glass, thin-fallback, and far-baked luminance/reflection
  comparisons; and
- an assertion that review provenance labels packed thickness unconsumed until
  the renderer actually samples it.

Tests should encode the rejected candidate-1 failure directly: excessive
periodic spectral energy at the prior 11/17-cycle bands, landmark correlation
across repeats, and unsupported runtime-thickness claims.

### Visual acceptance set

Capture both the procedural texture lab and a frozen tactical building fixture:

1. crown-disc source field with several quarry crops and process-direction
   overlays;
2. cylinder-sheet source field with several crops and its working axis;
3. clear, typical, and defect-rich quality grades at true metric scale;
4. a 2 by 2 texture repeat, seam diagnostics, and separated transmission,
   optical normal, physical thickness, and roughness maps;
5. a high-contrast exterior facade, fine checker, text-like detail, foliage,
   and moving character viewed through a fixed pane;
6. reflected sky/buildings at face-on and grazing angles;
7. diamond and square leaded windows with modeled cames, solder nodes, putty,
   and pane edges;
8. an operable casement closed, partly open, and fully open from inside/outside;
9. one, two, and several overlapping transparent windows to expose ordering;
10. direct-sun interior shadow compared with a no-glass reference;
11. every mip and building LOD under a slowly moving camera; and
12. fallback captures with refraction/transmission unavailable.

The GPU fixture must hold camera, opaque background, lighting, exposure,
resolution, and tone mapping constant. Capture a no-glass reference, because a
beautiful standalone pane says little about whether scene-color transmission is
correct. Include video or a dense deterministic frame sequence: temporal
swimming and sorting errors are not reliably visible in a still.

Acceptance questions:

- Does a crown pane show fragments of a shared radial process rather than a
  bull's-eye stamped into every quarry?
- Does cylinder glass show restrained coherent directionality rather than water
  ripples or machine corrugation?
- Is ordinary background detail legible but gently distorted?
- Do clear regions exist, and are bubbles sparse enough not to read as seeded
  decorative glass?
- Does tint arise through transmission without turning the pane opaque blue or
  emissive?
- Do surface normal, thickness, roughness, and absorption produce distinct but
  coherent effects?
- Are cames, edges, putty, frames, bars, hinges, and latches real assembly
  features?
- Does the casement remain optically and mechanically coherent while moving?
- Do transparent ordering, shadows, and overlapping panes remain stable?
- Do distant windows simplify without popping, sparkling, vanishing, or becoming
  blue holes?

An independent reviewer should receive shuffled crown/cylinder candidates,
metric/process manifests, and the no-glass references. Approval requires correct
craft identity, optical behavior, assembly ownership, and temporal/LOD
stability; a convincing grayscale normal map alone is insufficient.

## Common failure modes to reject

- modern float glass with a generic water-noise normal;
- one texture that mixes radial crown rings and parallel cylinder cords on every
  pane;
- a centered bull's-eye in every quarry or facade opening;
- dense identical bubbles used as the sole signal for “old”;
- universal sinusoidal corrugation or repeated high-contrast landmarks;
- millimetre feature claims at a 4.69 mm base texel;
- every pane remapping the complete 2.4 m repeat into local 0-1 UVs;
- world-projected features continuing uninterrupted through lead cames;
- opaque blue/green base color standing in for absorption and transmission;
- alpha treated simultaneously as opacity, transmittance, and shadow density;
- IOR reduced toward 1 to compensate for an over-strong optical normal;
- optical height copied directly to roughness and thickness;
- packed thickness advertised as rendered while runtime uses only a scalar;
- lead came grids, putty, bars, latches, or pane cracks baked into the glass
  tile;
- zero-thickness double-sided planes sent through a solid-volume refraction
  model without a declared thin-walled approximation;
- coplanar front/back shells, transparent sorting errors, or opaque glass
  shadows;
- byte-averaged sRGB and encoded-normal mips;
- distant refraction shimmer and abrupt transparent-to-facade LOD pops; and
- a CPU distortion proxy accepted as proof of the actual runtime material.

## Bottom line

The most important next step is not to add more waviness. It is to give each
pane a historically coherent manufacturing identity and a truthful optical
pipeline. Crown glass needs virtual-disc coordinates and restrained radial
evidence; cylinder glass needs source-sheet coordinates and restrained parallel
evidence. Both require sparse bubbles, large quiet areas, physical thickness,
absorption-based tint, low roughness, and gentle optical distortion. Lead cames,
putty, edges, frames, bars, and operable casements belong to geometry and
pane-aware masks. Near windows need measured transmission/reflection and actual
runtime capture; distant windows need a stable baked representation. Semantic
mips and continuous LOD are essential because old-glass normals move the image
behind the pane and can otherwise become more distracting at distance than at
close range.
