# Lime plaster procedural texture prior art

## Scope

This report concerns exactly the `LimePlaster` procedural surface: a lime-rich,
sand-aggregate finish suitable for plaster infill and rendered façades. It
covers the relationship among plaster body, finish coat, and limewash; trowel
and float marks; exposed aggregate and pinholes; cracks, patches, staining, and
weathering; physically restrained albedo, roughness, normal, height, and AO;
physical scale; tiling and mip behavior; and how a reusable tile should be used
on whole façades.

It does not prescribe building colors, fachwerk layout, unique façade damage,
or wall geometry. Those are consumers of the base surface and need separate
spatial masks. It also does not treat Portland-cement stucco, gypsum skim coat,
or polished Venetian plaster as interchangeable with ordinary historical lime
plaster.

## Repository facts and constraints

The following are facts observed in this worktree, not claims from external
sources.

- `LimePlaster` is an implemented `Wall` recipe with albedo, OpenGL normal,
  height, and ARM outputs.
- It generates a deterministic 1024 by 1024 tile representing 1 metre square,
  or about 0.977 mm per source texel. The declared full normalized relief range
  is 4 mm.
- The current surface combines broad periodic noise, an 11-cell trowel field,
  oblique sweep modulation, 73-cell sand noise, sparse 128-cell aggregate
  features, and sparse 32-cell pinholes.
- Albedo uses three warm/cool buff values mixed continuously, with quantized
  mineral modulation, slight aggregate lightening, and strong pinhole
  darkening. It is encoded as sRGB.
- Height-derived normals use the declared physical tile and relief scale.
  Roughness is high (0.74–0.94), includes oblique micro variation, aggregate and
  pinhole effects, and becomes rougher on greater physical slopes. Metalness is
  zero. AO is currently a shallow formula from pinholes and negative height,
  not a multi-radius horizon calculation.
- All four maps have complete mip chains, repeat addressing, linear filtering,
  and anisotropy. The shared mip helper byte-averages each channel. In
  particular, it does not decode/renormalize normal mips, average albedo in
  linear light, or increase roughness to account for unresolved normal
  variance.
- Tests already prove determinism, periodicity, physical scale, relief range,
  sparse pinholes, bounded roughness behavior, nontrivial albedo palette,
  channel packing, correct albedo color space, and complete mip dimensions.
- The building model assigns `WallMaterialClass::TimberInfill` to both
  timber-frame and plaster walls. The tactical presentation additionally has
  `PlasterInfill` and `FullyRendered` appearance families.
- Despite that semantic match, tactical building materials currently create
  palette-colored checker textures for plaster infill and fully rendered walls.
  The generated `ProceduralTextureAssets::lime_plaster` maps are not bound to
  those façade materials. The procedural texture lab can review the recipe in
  isolation, but the current tactical city does not demonstrate its runtime
  façade result.
- The far shell LOD has a separate fachwerk-baked texture path. A base plaster
  material must therefore remain compatible with near-wall UV scale and with
  whatever composite or bake carries plaster into distant façades.

These constraints separate two tasks. The base recipe needs more causal
plaster structure and semantic mips; the building presentation needs a
deliberate way to tint and consume it. Improving only one will not improve the
visible city.

## Practitioner and conservation evidence

### Plaster is a layered assembly

Historic plaster and render are not one homogeneous noisy sheet. The National
Park Service's preservation brief describes a scratch coat keyed to masonry or
lath, successive drying between coats, and a separately prepared pigmented
finish coat. Repair guidance calls for matching the original number and
thickness of coats as well as composition, texture, and color
([NPS Preservation Brief 22](https://www.nps.gov/orgs/1739/upload/preservation-brief-22-stucco.pdf)).
Historic Environment Scotland's lime guide likewise identifies limewash as a
finish applied directly to masonry or, more commonly, over lime harling,
render, or plaster
([HES/Scottish Lime Centre guide](https://www.scotlime.org/documents/15/Technical_Advice_Guide_Introduction_to_Lime_in_Traditional_Buildings_1.pdf)).

The visible pixel material is therefore the top of an assembly:

1. wall or timber infill substrate;
2. coarse keyed base/body coats with larger aggregate and possible fibers;
3. finer finish coat worked by float or trowel;
4. optional repeated limewash coats, pigment, and later repairs.

**Evidence-backed conclusion:** a good procedural should not expose every
underlying layer everywhere. The intact finish coat dominates; body aggregate,
scratch keys, or substrate should appear only at genuine loss or deep repair.

**Inference for this repository:** keep `LimePlaster` as an intact base finish.
It may include sparse aggregate exposure and pinholes, but large spalls that
reveal wattle, brick, or masonry require a separate material-layer mask tied to
the correct wall construction. Do not bake generic dark “holes” into every
plaster tile and imply a substrate that may be wrong.

### Limewash and integral plaster color are not ordinary paint

The NPS brief notes that traditional limewash bonds with lime stucco, was often
renewed, and can be used after repairs. It also warns that water amount,
overworking, and pigment batching affect the dried color
([NPS Preservation Brief 22](https://www.nps.gov/orgs/1739/upload/preservation-brief-22-stucco.pdf)).
The guide's recommendation to weather test patches through changing seasons
underscores that tint and surface response are material/time effects, not one
flat RGB value.

Historic England's defect guide associates white surface crust or laitance
with rapid drying or excessive working that draws lime to the surface. It also
records light zones around shrinkage cracks caused by lime migration
([Historic England, *Mortars, Renders & Plasters*](https://historicengland.org.uk/images-books/publications/mortars-renders-plasters-conservation/mortars-marketing-spread)).

**Inference for this repository:** the existing warm/cool buff palette is a
reasonable restrained base, but coloration should be split into semantic
layers:

- finish-body mineral color, slowly varying and low contrast;
- limewash veil, lighter and more chromatically restrained, with subtle brush
  overlap or wear;
- sparse aggregate influence, visible chiefly where the surface is open;
- repair-patch color, slightly different but locally coherent;
- environmental staining supplied by façade-space masks rather than the
  repeating base tile.

Albedo must remain free of directional lighting, crevice-black AO, and generic
“grunge.” Limewash is matte and optically variable, but that does not justify
large random black and white blotches.

### Trowel marks are strokes, not isotropic noise

A practitioner cottage-wall breakdown uses anisotropic noise masked by brush
patterns distributed with random rotation to create the subtle directional
swipes of applied plaster. Those grayscale structures drive color, normal,
roughness, AO, and height, with worn versions adding cracks, crumbled sections,
and staining as separate changes
([Wilson, ArtStation](https://www.artstation.com/blogs/toadstl/Z4rrz/rendering-a-corner-of-a-workers-cottage)).

The old Houdini Stucco VOP is simple but makes two relevant points: stucco is a
light, bumpy finish, and displacement amplitude is an explicit control; its
pattern is anti-aliased rather than unlimited high-frequency noise
([SideFX Stucco VOP](https://www.sidefx.com/docs/houdini/nodes/vop/stucco.html)).
SideFX's general shader guidance treats surface and displacement as distinct
material outputs that are composed through a graph
([SideFX material-building documentation](https://www.sidefx.com/docs/houdini/shade/build.html)).

**Evidence-backed conclusion:** plaster application produces overlapping,
finite strokes with direction, length, width, pressure, and edge behavior.
Random noise can break them up but should not be the primary trowel model.

**Inference for this repository:** replace or augment the 11-cell scalar
“trowel” noise with a periodic stroke field:

- scatter elongated quadratic or capsule strokes, perhaps 80–350 mm long and
  20–100 mm wide, with a restrained orientation mixture;
- use a shallow crown/trough pair across the stroke and tapered endpoints;
- overlap strokes in a deterministic top order or soft maximum so later passes
  partially flatten earlier ones;
- let pressure alter width and sub-millimetre amplitude, not color directly;
- reserve a few broader float sweeps for low-frequency undulation;
- ensure no stroke edge becomes a centimetre-deep groove.

The visible finish for ordinary 1544 walls should generally be quiet. Trowel
marks are a close-range grazing-light cue, not a repeating decorative motif.

### Aggregate, pores, and surface finishing must share scale

Traditional lime plaster is lime, aggregate (commonly sand), and water; the NPS
plaster-preservation project describes repair with lime, aggregate, and water
([NPS Monocacy plaster project](https://home.nps.gov/mono/learn/historyculture/plaster-preservation-project.htm)).
Conservation guidance also emphasizes matching composition and texture rather
than substituting a hard modern cement-rich finish
([NPS Preservation Brief 22](https://www.nps.gov/orgs/1739/upload/preservation-brief-22-stucco.pdf)).

**Inference for this repository:** at roughly 0.98 mm/texel, a 1024² metre tile
can represent coarse sand and small pinholes, but not every fine grain as a
separate height spike. Use three roles:

- 1–4 mm aggregate: mostly roughness and restrained albedo flecks, with only
  the largest exposed grains contributing stable normal relief;
- 3–15 mm pinholes/pulls: sparse negative height with rounded, asymmetric
  boundaries and actual local AO;
- 10–80 mm application texture: float/trowel strokes and small local
  undulation.

Aggregate distribution should be coupled to finishing: a polished or
lime-rich worked surface exposes less coarse sand; abrasion or weak patches
expose more. The current 128-cell aggregate field averages 7.8 mm cells, which
is plausible for aggregate clusters but too large if each cell reads as a
single sand grain. Name and test it as a cluster/exposure mask unless its
physical shape becomes truly granular.

### Crack morphology should encode a cause

Ty-Mawr's troubleshooting guide lists distinct causes for lime-plaster cracks.
Over-trowelling can bring too much lime “fat” to the surface, leaving
insufficient aggregate and causing shrinkage; moisture and carbonation control
also matter; some uniform cracks follow reactive mortar beds
([Ty-Mawr, lime-plaster cracking](https://www.lime.org.uk/knowledge-base/application---lime-plaster-cracking/)).
Historic England distinguishes fine crazing from overworking, wandering
multidirectional shrinkage cracks, directional cracks over concealed elements,
diagonal sill cracks, and cracks accompanied by lime migration or rust staining
([Historic England, *Mortars, Renders & Plasters*](https://historicengland.org.uk/images-books/publications/mortars-renders-plasters-conservation/mortars-marketing-spread)).

The USG plaster glossary independently describes craze cracks as fine random
finish-layer fissures from shrinkage and trowel chatter as ripples from a dry
surface that no longer lubricates the tool
([USG plastering glossary](https://www.usg.com/content/dam/USG_Marketing_Communications/united_states/product_promotional_materials/finished_assets/plastering-technical-guide-pm-glossary-en-PM1.pdf)).

**Evidence-backed conclusion:** one Voronoi crack network cannot stand in for
all age and damage. Crack scale and direction indicate process and wall
context.

**Inference for this repository:** the base tile should contain at most very
sparse hairline craze cracking, and even that should be a selectable weathering
variant. Larger features belong to façade masks:

- broad, shallow random craze patches for finish shrinkage;
- diagonal cracks emanating from opening corners;
- horizontal or vertical cracks aligned with concealed timber/masonry joints;
- wandering cracks linked to movement or poor bonding;
- local spalls/delamination around water ingress or failed patches.

These need wall dimensions, openings, fachwerk, roof runoff, and ground level.
They cannot be positioned plausibly inside a 1 m endlessly repeated texture.
The base recipe should expose semantic masks/helpers usable by a later façade
weathering compositor, rather than baking every failure into albedo.

### Repairs are layered material transitions

Procedural material artists typically build damaged plaster as a height-layer
problem. Rosen Kazlachev's wall graph makes an art-directed plaster height,
hand-placed damage holes, separate masks for top and middle plaster, and then
height-blends plaster, grout, and brick. The same masks are reused for color and
weathering, while directional blur adds gravity to leaks
([Kazlachev, ArtStation](https://www.artstation.com/blogs/rosko/YjnN/free-material-tutorial-arcane-brick-wall-in-substance-designer)).
Mike Spadaro's damaged wall is explicitly based on plaster loss revealing the
brick and mortar underneath
([Spadaro, ArtStation](https://mjspadaro.artstation.com/projects/58bdrP)).

NPS preservation practice requires repairs to match the original mix, number
and thickness of coats, texture, and color, while acknowledging that small
patches can remain conspicuous
([NPS Preservation Brief 22](https://www.nps.gov/orgs/1739/upload/preservation-brief-22-stucco.pdf)).

**Inference for this repository:** represent repairs with masks and edge
profiles, not as random color islands:

- a patch has a coherent shape and scale, often following removed loose work;
- its center is a compatible plaster variant;
- its feathered or cut edge changes height/normal;
- the patch's pigment and aggregate may differ subtly from adjacent aged work;
- limewash may cross both old and new plaster, partly reducing the color seam;
- exposed substrate appears only where the plaster layer is actually absent.

This should be a later façade-level overlay. The `LimePlaster` base can provide
two or three deterministic intact finish variants for old surface, repair, and
fresh limewash.

### Weathering follows architecture and exposure

Historic England identifies specific environmental patterns: surface staining
from organic growth, salts, and carbon; dark soiling in sheltered zones;
streaking from irregular water channels at joints and copings; and damp-related
failure at parapet gutters, ground level, and openings
([Historic England, *Mortars, Renders & Plasters*](https://historicengland.org.uk/images-books/publications/mortars-renders-plasters-conservation/mortars-marketing-spread)).

The practitioner Arcane-wall workflow similarly derives downward leaks from
occluding damage and uses directional blur for gravity rather than applying
isotropic stains everywhere
([Kazlachev, ArtStation](https://www.artstation.com/blogs/rosko/YjnN/free-material-tutorial-arcane-brick-wall-in-substance-designer)).

**Inference for this repository:** divide weathering by coordinate system:

- **Tile/local material:** mineral mottling, tiny pores, application marks,
  sub-centimetre aggregate, faint diffuse wear.
- **Façade/object:** sill runoff, eave shelter, splashback at ground, corner
  exposure, opening cracks, timber-joint lines, patch boundaries.
- **World/environment:** prevailing rain, damp/shade, orientation, proximity to
  streets and soil, settlement age and maintenance state.

Do not generate a strong downward streak inside the base tile unless façade UVs
guarantee world vertical and the streak phase is broken across the whole wall.
A one-metre repeating drip is more artificial than a clean wall.

### Albedo, roughness, normal, height, and AO need restrained relationships

Adobe's PBR guidance classifies stone-like mineral finishes as dielectrics with
a diffuse component and mostly untinted specular reflection. Roughness changes
highlight shape; metalness should not be used as a wear or dirt control
([Adobe OpenPBR overview](https://experienceleague.adobe.com/en/docs/substance-3d/general-knowledge/openpbr/openpbr-overview)).
Adobe also specifies that normal, roughness, displacement, and AO are non-color
data, while display-referred base color requires the proper color transform
([Adobe color management](https://experienceleague.adobe.com/en/docs/substance-3d/ecosystem/renderers/color-management/color-management)).

Production material systems benefit from separating reusable tileable surface
layers from asset masks. The Callisto Protocol talk describes tileable base
layers combined through tile masks to preserve high texel density while making
asset variants
([Juarez, GDC 2023](https://gdcvault.com/free/gdc-23/play/1029338/Adobe-Developer-Summit-The-Callisto)).
The Order: 1886 material pipeline likewise uses inheritance, offline
compositing, and runtime layer blending to keep response consistent across a
large body of content
([Neubelt and Pettineo, GDC 2014](https://gdcvault.com/play/1020485/Crafting-a-Next-Gen-Material)).

**Inference for this repository:** preserve correlation where causal, but
reject channel cloning:

- **Height/normal:** application relief, aggregate exposure, pinholes, and tiny
  finish cracks. Normal is derived from height in metres.
- **Albedo:** lime/aggregate/pigment composition and thin limewash variation.
  Cavity shading remains out of albedo.
- **Roughness:** generally high, but a freshly worked/lime-rich trowel sheen may
  be slightly smoother; exposed sand and powdery weathered work may be rougher;
  dampness, if modeled, needs an environment mask and typically darkens albedo
  while reducing roughness.
- **AO:** computed from local relief over a small physical radius. Fine
  hairlines and shallow color mottles should contribute little or none.
- **Metalness:** zero everywhere.

The current implementation's correlation between slope and slightly increased
roughness is plausible only as an exposed-aggregate proxy. It should not be a
universal rule that every trowel ridge is rougher than every flat.

## Physical scale and relief budget

The current 1 m/1024² scale is generous and appropriate for a close façade
material. Keep it explicit. A recommended allocation inside the existing 4 mm
normalized relief envelope is:

| Feature | Width/spacing | Typical height effect | Primary channels |
|---|---:|---:|---|
| Broad finish undulation | 150–600 mm | 0.3–1.2 mm | height, normal |
| Trowel/float stroke | 20–350 mm | 0.1–0.8 mm | height, normal, slight roughness |
| Aggregate cluster/exposure | 5–30 mm | 0.1–0.6 mm | roughness, albedo, normal |
| Individual coarse grain | 1–4 mm | 0–0.3 mm | roughness, restrained normal/albedo |
| Pinhole or pull | 3–15 mm | −0.5 to −2 mm | height, normal, AO |
| Hairline craze crack | 0.2–1 mm | mostly sub-texel | albedo/roughness; limited normal |

These are target authoring bands, not historical measurements asserted by the
sources. They should be tuned against captured/reference surfaces and the
actual tactical camera. Features narrower than one texel cannot be stable
binary height shapes. Use analytic coverage or omit them from relief.

The full 4 mm range should be reserved for rare pinholes and pulls. If ordinary
trowel noise routinely spans it, the surface will read as roughcast or eroded
concrete rather than a worked lime finish.

## Tiling, UVs, and façade use

### The base tile must be quiet enough to repeat

Procedural texturing is well suited to coordinating bump, specularity, and
effect maps, but the classic practitioner warning is that familiar fractal
noise becomes visual clip art. Steve Theodore describes procedurals as a
layered construction—base, weathering, stains, cracks—rather than a single
magic noise
([Theodore, *Game Developer*](https://media.gdcvault.com/GD_Mag_Archives/GDM_November_2003.pdf)).

**Inference for this repository:** exact periodicity is necessary but not
sufficient. Test for memorable landmarks: the same deep pinhole group, bright
mineral island, or sine sweep should not reappear every metre across a long
wall. Prefer many low-contrast strokes and sparse features whose autocorrelation
does not form a grid.

Keep the base tile deterministic and exactly seam-free in value and gradient.
Break repetition at façade scale with coherent, cheap controls:

- per-building tint within curated lime/pigment palettes;
- per-wall UV phase and optional 90°-safe transform, while preserving vertical
  cues;
- a low-frequency façade mask for maintenance age, patching, runoff, and soil;
- optional selection among a few seed-wedged intact finish variants;
- LOD composites generated from the same surface/palette contract.

Do not offset albedo, normal, height, and ARM independently. Do not rotate a
gravity-directed field sideways merely to hide tiling.

### UVs should preserve metric scale and wall continuity

The user requirement for joined continuous wall meshes makes UV continuity
especially valuable: metre-scaled planar UVs can continue through mesh joins
without restarting the pattern on every panel. Openings and returns can have
their own chart logic, but adjacent coplanar wall sections should share the same
world/facade origin.

**Inference for this repository:** add acceptance fixtures with long joined
walls, corners, doors, windows, fachwerk bays, dormers, and LOD transitions.
Verify:

- one metre of UV corresponds to one metre of wall in every building size;
- the tile does not restart at former wall-segment boundaries;
- corner turns do not stretch the material;
- revealed jambs/returns have plausible scale;
- fachwerk and plaster meet without gaps or z-fighting;
- shell-LOD plaster color and aggregate response converge toward the same
  distant mean as LOD0.

### Wiring is a prerequisite for visual acceptance

The existing tactical material setup currently ignores the generated
`lime_plaster` surface and builds checker-textured infill materials. Therefore
texture-lab acceptance alone cannot establish success in the city.

**Inference for this repository:** a later implementation should create a
façade material that samples the generated albedo, normal, and ARM while
applying the selected `BuildingAppearance` palette as a restrained tint or
limewash layer. It must retain the current deterministic appearance assignment
and separate brick infill from plaster. Height can remain review/bake data
unless the runtime adds parallax or displacement.

For far LODs, bake or filter the plaster and fachwerk together with semantic
mips. Do not overlay a high-frequency plaster normal on a tiny façade where its
aggregate is below a pixel.

## Mip and distance behavior

The current full mip chain is a good starting contract, but ordinary RGBA box
averaging is not correct for every channel. Valve's VR presentation explicitly
shows that averaging normal maps loses important roughness information
([Vlachos, GDC 2015](https://media.gdcvault.com/gdc2015/presentations/Vlachos_Alex_Advanced_VR_Rendering_V2.pdf)).

**Inference for this repository:** generate semantic mips together:

- average albedo in linear light, then encode the mip as sRGB;
- decode, average, and renormalize normals;
- increase roughness according to unresolved normal variance so distant
  plaster does not become smoother and sparkle;
- average AO toward the mean local visibility, not toward a dark cavity color;
- reduce height amplitude as features become unresolved while preserving the
  mean plane;
- analytically filter subpixel pinholes/hairlines by coverage or let them fade,
  rather than letting them flicker between black and absent.

At distance, the surface should converge to a stable warm/cool matte field.
Trowel strokes and individual grains disappear before the façade's broad
limewash and weathering zones. No mip should shift the wall substantially
darker, shinier, warmer, or cooler.

## Recommended bounded implementation

The following proposal is inferred for this repository.

1. **Retain the current public physical constants initially.** A 1 m tile,
   1024² source, and 4 mm rare relief envelope are adequate.
2. **Replace scalar trowel noise with finite periodic stroke primitives.** Keep
   broad low-amplitude undulation and sand breakup, but give application marks
   length, width, direction, taper, and overlap.
3. **Rename or remodel aggregate as clusters/exposure.** Couple exposed
   aggregate to finish wear and roughness; keep individual sub-2 mm grains out
   of explicit height.
4. **Improve AO.** Use a small deterministic multi-radius horizon/cavity
   estimate from physical height. Keep it shallow for a mostly flat finish.
5. **Separate intact surface from damage.** The base may contain sparse
   pinholes and perhaps a low-density craze variant; large cracks, repairs,
   spalls, substrate exposure, and runoff wait for façade-level masks.
6. **Create semantic mips.** At minimum, linear-light albedo, renormalized
   normals, and normal-variance roughness replace byte averaging.
7. **Expose controlled finish variants.** For example: ordinary floated,
   smoother lime-rich/troweled, and weathered aggregate-exposed. They should
   share one physical model and differ within bounded parameters.
8. **Wire the accepted texture into tactical plaster materials.** Combine it
   with existing deterministic appearance palettes rather than replacing
   façade color variety with one tan texture.
9. **Carry the same mean response into LOD composites.** The fachwerk-baked
   shell should not revert to a checker or unrelated plaster color.
10. **Profile before adding more runtime layers.** A façade-space weather mask
    may be worthwhile, but correctness of the base texture and UV continuity
    comes first.

## Deterministic acceptance plan

### Generator invariants

- Repeated generation is byte-identical.
- Sampling is periodic in value and first derivative across all tile edges.
- All images have 1024² base levels, complete mip chains, repeat/linear sampler
  state, and correct color-space formats.
- Metalness is exactly zero and alpha exactly opaque.
- Decoded normals are unit length within a defined tolerance at every mip.
- Height is bounded by the declared 4 mm range.
- Roughness remains high and bounded but has material-driven nonzero variance.
- AO correlates with computed local cavity/horizon visibility more strongly
  than with signed absolute height.

### Structural metrics

- Measure trowel stroke length, width, orientation, amplitude, and density in
  metres. Reject an isotropic field or one dominant repeated sweep.
- Measure aggregate-cluster and pinhole size distributions. Reject sub-texel
  binary features and regularly spaced cellular dots.
- Partition height energy into broad finish, strokes, aggregate, and pinholes.
  Ordinary trowel/stroke relief must not consume the rare 4 mm extremes.
- Measure channel correlations. Normal should correlate with height gradient;
  AO with cavities; albedo and roughness should share semantic masks without
  becoming copies of height or one another.
- Compute 2D autocorrelation and a 3 by 3 tile image. Reject obvious one-metre
  landmarks even when the seam itself is exact.

### Channel and mip review

- Capture albedo-only, height, normal, roughness, AO, and neutral-lit beauty
  panels at full resolution and every tactically relevant mip.
- Compare each mip's mean albedo (in linear light), mean roughness, and decoded
  normal length against explicit tolerances.
- Use a grazing moving light and slow camera dolly. Reject normal sparkle,
  roughness crawl, pinhole flicker, or a mip transition that makes the wall
  smoother or darker.
- Verify cracks/pinholes lose contrast gracefully instead of persisting as
  isolated black pixels.

### Façade fixtures

- A long uninterrupted plaster wall viewed frontally and at grazing angle.
- A timber-frame wall with multiple bays, windows, a door, and joined mesh
  sections.
- A fully rendered masonry façade with corners, sill/eave zones, and repair
  masks disabled and enabled.
- The same building at LOD0, LOD1, and LOD2 under fixed daylight and overcast
  lighting.
- A 5 by 5 deterministic group of plaster buildings using the curated city
  appearance distribution.

Review close, middle, and whole-city distances. An independent reviewer should
confirm that the close surface reads as worked lime plaster, the middle façade
reads as coherent rather than tiled, and the far building preserves color and
fachwerk without aggregate shimmer.

## Common pitfalls to reject

- Generic fractal noise labeled “stucco.”
- Trowel marks made from isotropic blobs or endless sine waves.
- Centimetre-deep ordinary strokes within a 4 mm finish contract.
- Every sand grain represented as a normal spike.
- Uniform cellular pinholes or perfect Voronoi cracks.
- One crack network combining shrinkage, structural movement, timber joints,
  and weather damage.
- Revealing brick or wattle beneath every wall regardless of construction.
- Baking AO, drip shadows, or dark dirt into base albedo.
- Repeating one downward stain every metre.
- Placing sill/eave/ground weathering inside a generic tile rather than façade
  space.
- Making all relief slopes rougher regardless of finish or wear.
- Independent random phase for albedo, normal, and ARM.
- Byte-averaged normal mips and sRGB albedo mips.
- A fresh repair patch that differs only by a hard-edged color decal.
- Using texture-lab beauty renders as proof while the tactical renderer still
  binds checker textures.
- Changing building palette variety into one universal cream wall.
- Letting LOD plaster converge to a different mean color or a noisy normal.
- Solving large plaster loss with parallax where geometry/material layering is
  required.

## Source assessment

The NPS, Historic England, and Scottish conservation sources are used for the
physical layering of lime plaster/limewash and for cause-specific repair,
cracking, staining, and weathering patterns. They are stronger evidence for
material causality than modern decorative-plaster marketing. The ArtStation
breakdowns are first-person procedural-material evidence for trowel strokes,
height layering, damage masks, substrate reveals, and channel reuse. SideFX
sources establish anti-aliased procedural relief and graph composition. GDC
material-pipeline talks support separating reusable tile layers from façade or
asset masks and preserving channel behavior at runtime; Valve's presentation
specifically supports semantic normal/roughness mips.

Repository facts are isolated at the beginning. Recommendations and proposed
physical bands are explicitly marked as inference; no cited practitioner is
claimed to prescribe this exact Rust, Bevy, UV, or LOD implementation.
