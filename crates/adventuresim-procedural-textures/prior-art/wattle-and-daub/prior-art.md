# Wattle-and-daub procedural texture prior art

## Scope

This report concerns exactly the `WattleAndDaub` procedural surface intended
for fachwerk infill panels. It covers the hidden woven substrate, earth-and-
fibre daub, optional lime-plaster/limewash finish, localized damage that exposes
the construction, causal relationships among height, normal, roughness, AO,
and albedo, physical scale, façade UVs, tiling and mip behavior, and LOD use.

It does not prescribe the surrounding timber structure, whole-building color
distribution, or structural failure geometry. It also does not assume every
German infill panel used one recipe: earth, fibres, wattle species, surface
finish, masonry replacement, and regional practice varied. The texture should
represent one controlled material family, not label all `TimberInfill` walls as
identical wattle and daub.

## Repository facts and constraints

The following are facts observed in this worktree, not claims from external
sources.

- `WattleAndDaub` is an implemented `Wall` recipe producing albedo, OpenGL
  normal, height, and ARM maps.
- It generates a deterministic 1024 by 1024 tile representing 1.5 metres
  square, about 1.465 mm per source texel. The declared full normalized relief
  range is 12 mm.
- The present intact surface combines broad, medium, and fine periodic noise;
  warped smears; two sinusoidal trowel fields; sparse aggregate; sparse fibres;
  and sparse capsule-shaped shrink cracks.
- One small exposed cavity is fixed near UV `(0.22, 0.31)`. Inside it, one
  diagonal capsule suggests a woody fragment, but the recipe does not construct
  a repeating stave-and-withe weave beneath the daub.
- Albedo mixes cool and warm brown earth, dark aggregate and fibres, cracks,
  and the exposed cavity. It does not currently include a distinct pale finish
  coat or limewash layer.
- Roughness is very high (0.80–0.96), metalness is zero, and AO uses a
  multi-distance cardinal height comparison plus extra cavity/crack darkening.
- Tests prove deterministic periodic sampling, approximate value and
  first-derivative edge continuity, the declared physical scale, restrained
  sparse-feature coverage, nonmetalness, color-space declaration, channel
  variation, and complete mip dimensions.
- The shared mip generator averages encoded bytes. It does not average sRGB
  albedo in linear light, decode/filter/renormalize normals, derive roughness
  from unresolved normal variance, or preserve the semantic coverage of tiny
  exposed-wattle cavities.
- High-detail exterior wall UVs use a 2 metre physical repeat based on wall
  tangent and height, whereas this recipe declares a 1.5 metre tile. A material
  binding must use the recipe's scale or explicitly transform UVs; otherwise
  all features will be one-third too large.
- The building model maps both `TimberFrame` and `Plaster` wall styles to
  `WallMaterialClass::TimberInfill`. That class alone cannot prove that a panel
  has a wattle substrate rather than another infill or that its exterior finish
  should expose raw daub.
- Tactical building presentation currently gives `TimberInfill` a palette-
  colored checker texture. Although `ProceduralTextureAssets` contains
  `wattle_and_daub`, the tactical material setup does not bind it to façades.
- The far shell uses a separate palette-generated `FachwerkBaked` texture. It
  does not inherit this recipe's surface statistics or its localized damage.

Consequently, improving the recipe does not by itself improve the visible
city. Runtime needs a deliberate infill/finish choice, a physical UV-scale
contract, and an LOD composite policy.

## Historical and conservation evidence

### The weave is a substrate, not normally the finished façade

German fachwerk infill could use stakes and woven rods covered with earth.
Training material from the German craft academy describes oak stakes, woven
willow rods, and a thrown earth coat, while emphasizing that many regional
infill techniques existed
([Akademie des Handwerks, *Fachwerkausfachung und Lehmbau-Techniken*](https://www.akademie-des-handwerks.de/wp-content/uploads/fachwerksausfachung-lehmbautechniken.pdf?target=blank)).
A German environmental-foundation report describes panels originally filled
with oak splitwood and willow rods, daubed and smoothed on both sides, and later
given a thin lime-plaster layer
([DBU, Quedlinburg fachwerk report](https://opac.dbu.de/ab/DBU-Abschlussbericht-AZ-23723.pdf)).
ClayTec's restoration worksheet describes the structural principle: pointed
stakes are fixed into grooves, spaces are woven or directly closed with
straw-earth, and a lime/fine-sand slurry prepares historic clay infill for a
compatible exterior coat
([ClayTec, *Timber-frame restoration worksheet*](https://claytec.de/wp-content/uploads/2022/03/Arbeitsblatt_Fachwerk_02-2021_EN.pdf)).

Building Conservation's wattle-and-daub guidance likewise treats daub as the
infill body and soft, porous haired lime plaster plus limewash as protective
finishes. It notes that exposed daub is vulnerable and that intact exterior
panels should shed water while remaining permeable
([Hunt, *Wattle and Daub*](https://www.buildingconservation.com/articles/wattleanddaub/wattleanddaub.htm)).
The detailed conservation manual says unprotected daub is essentially exposed
soil and that limewash both sheds weather and binds the surface through calcium
carbonate
([Graham et al., *Wattle & Daub: Craft, Conservation and Wiltshire Case Study*](https://www.tonygraham.co.uk/house_repair/Wattle_Daub_Conservation.pdf)).

**Inference for `WattleAndDaub`:** the default intact exterior should read as a
continuous smoothed daub or lime-plastered/limewashed panel. The wattle weave
should be absent from intact albedo and normals. It becomes visible only where
finish and daub have genuinely detached or eroded through several centimetres.
A regular dark basket pattern across an undamaged panel would describe an
excavated or ruined wall, not an ordinary inhabited façade.

### The panel is a layered assembly

The conservation manual identifies construction stages and regionally varying
ingredients: stakes/wattle; daub containing local earth and strengthening
fibres; sometimes a keyed surface; a plaster coat that can contain lime, sand,
hair, straw, or other local additions; and limewash. Its repair instructions
call for matching the original texture, priming exposed daub, applying
compatible haired plaster, finishing with a wooden float, and limewashing after
drying. Building Conservation likewise recommends matching original material
and explains that lime plaster fills cracks and gaps while retaining
permeability.

This layered reality closely matches procedural-material practice. Daniel
Thiger's parameterized plaster-wall workflow builds plaster and damage as
separate controllable systems, using masks to expose underlying structure
([Level Up Digital, *Plaster Wall with Parameter-driven Bullet Holes*](https://levelupdigital.artstation.com/projects/2xOkNY)).
Lorena Da Silva Pinzón's procedural quincha material is a particularly relevant
practitioner analogue: a wood framework covered with mud/straw and finished
with plaster is authored as a procedural material in Substance Designer
([Da Silva Pinzón, *Quincha Wall Material*](https://lorelai.artstation.com/projects/eaZbnJ)).
Quincha is not German fachwerk, so it is not historical evidence for the game;
it is evidence that technical artists productively decompose this class of
surface into substrate, earthen body, and finish.

**Inference:** construct three causal height/material layers:

1. an underlying stave-and-withe weave with real interlacing depth;
2. a thick daub body containing fibres and aggregate, with trowel/hand-smoothed
   macroform and drying shrinkage;
3. an optional thin finish coat/limewash with its own float marks, pores,
   patches, and restrained color.

Damage masks should remove these layers in order. A shallow chip removes
limewash or finish coat, a deeper spall exposes coarse daub and fibre, and only
a rare deepest cavity exposes wattle. This avoids the implausible current
transition from intact earth directly to one isolated wooden capsule.

### Wattle construction has organized, load-bearing rhythm

The craft evidence describes stiff vertical stakes held in timber grooves and
more flexible rods woven through them. The weave is therefore not isotropic
random sticks. Stakes should be relatively straight and panel-height; withies
should alternate in front of and behind successive stakes, bowing slightly
between them. Interlacing changes height and occlusion at each crossing.

**Inference:** generate the substrate in panel-local space, not as a generic
seamless material. Its spacing should relate to the individual bay width and
height, and terminate at the surrounding frame. A tile can provide a reusable
damaged patch, but a convincing large exposed area requires the generator or a
panel-specific bake to know panel boundaries. Stable per-panel seeds may vary
rod diameter, bow, spacing, and missing fragments without losing the basic
vertical-stake/horizontal-weave organization.

The current 1.465 mm texel can represent centimetre-scale rods, straw, coarse
aggregate, cracks, and float marks. It should not attempt explicit sub-
millimetre clay grains. Fine mineral granularity belongs in bounded
albedo/roughness statistics and should disappear early in mips.

### Cracks are caused by shrinkage and frame movement

Building Conservation notes that some shrinkage is normal and that gaps around
panel edges commonly result from both daub shrinkage and timber seasoning. It
also explains that brittle, impervious cement repairs crack especially at the
timber junction and trap water. German fachwerk practice similarly recognizes
movement: the infill is stressed by seasonal timber swelling, and exterior
plaster can bulge or detach
([Bauhandwerk, *Lebendige Gefache: Lehmvarianten in der Fachwerksanierung*](https://www.bauhandwerk.de/artikel/bhw_Lebendige_Gefache-1734603.html)).
The conservation manual distinguishes ordinary drying cracks from major
delamination and notes that large new-daub cracks can reflect overly
shrinkable clay, too many fines, or too much water.

**Inference:** crack placement should be hierarchical and façade-aware:

- fine drying cracks form sparse, branching or curved networks in the daub;
- stronger separation concentrates at the timber/infill perimeter;
- local cracks can radiate from fibre clumps, previous patches, or impact;
- bulging and delamination form broad low-frequency height changes before a
  deep cavity appears.

A repeating tile cannot know where the timber perimeter is, so edge gaps and
frame-contact cracking belong to a panel mask generated from façade geometry.
The base recipe should retain only restrained interior shrinkage. SideFX
practitioners recommend curve-based crack patterns when controlled grooves are
needed rather than accepting raw Voronoi cell borders
([SideFX forum, *How to make stylized, groove-like cracks procedurally?*](https://www.sidefx.com/forum/topic/83631/)).
That is a stylized example, but the transferable technique—construct crack
paths, then vary their width/depth—is more controllable than undirected noise.

### Surface channels must agree without becoming copies

All layers are dielectric, so metalness remains zero. Intact limewash can be
pale and comparatively even but should still be matte; coarse raw daub,
exposed fibres, cavity walls, and weathered wood are rougher. Compact or
trowel-burnished high points may be slightly smoother. Dampness darkens albedo
and can reduce apparent roughness, but it should be a building/weather mask,
not a permanent property repeated every 1.5 metres.

The causal order should drive every map:

- broad trowel/float strokes and bulging affect height and normals strongly,
  albedo weakly, and roughness moderately;
- aggregate and fibres affect albedo/roughness and only sufficiently large
  pieces affect height;
- cracks lower height, darken through occlusion rather than black pigment, and
  become rougher at broken edges;
- a removed finish coat changes albedo and roughness before exposing deep
  height;
- a true cavity creates strong parallax/normal/AO and reveals coherent woven
  geometry beneath.

GDC material-layer practice supports deriving surface instances from reusable
base materials and applying separate damage/effect layers rather than baking
all conditions into one monolithic bitmap
([Pettineo, *Crafting a Next-Gen Material Pipeline for The Order: 1886*, GDC 2014](https://media.gdcvault.com/GDC2014/Presentations/Pettineo_Matt_Crafting_A_Next-Gen.pdf)).
The recommendation here is an inference adapted to the repository's offline
procedural generation; it does not require a runtime material-layer shader.

## UVs and façade integration

For intact daub/plaster, wall tangent and vertical height are suitable UV axes.
They must be expressed in metres and scaled to the recipe's declared 1.5 metre
tile. Adjacent triangles in one continuous façade run should share coordinates
to prevent seams and stretched textures around openings.

The deeper construction needs panel coordinates:

1. the substrate's vertical stakes run from sill to plate within one framed
   bay;
2. woven rods cross the bay and alternate depth at each stake;
3. exposed-damage masks stay inside that bay and understand distance to the
   timber boundary;
4. per-panel seeds remain fixed across runs and LOD changes;
5. a panel-specific projection or bake preserves the same exposed patch on
   front and, where relevant, a different finish on the interior face.

Do not project a weave continuously through posts, windows, doors, or from one
bay into the next. For cheap intact façades, the panel-local substrate can be
omitted entirely until a damage variant selects it.

The semantic material model also needs refinement. `WallStyle::Plaster` should
not automatically imply visible wattle and daub, and `TimberFrame` only proves
the frame style, not the exact infill construction. A clean final schema should
represent infill substrate and finish separately—for example, wattle/daub body
plus raw-daub, lime-plastered, or limewashed finish—rather than adding a legacy
fallback keyed only from `TimberInfill`.

## Tiling, mips, distance, and LODs

The fixed cavity is a strong repetition landmark. At a 1.5 metre repeat it can
appear once in nearly every panel or in a visible façade grid. Deep exposure
should instead be a sparse deterministic panel-level event with a small
variant library or generated masks. The intact base surface can tile, but broad
trowel direction and color should receive low-frequency per-panel variation.

Mip generation must respect channel semantics:

- downsample albedo in linear light and re-encode to sRGB;
- decode, filter, and renormalize normal vectors;
- incorporate unresolved normal variance into roughness so coarse mips do not
  become unnaturally smooth;
- filter height without letting a single deep cavity lower an entire panel;
- handle damage-mask coverage explicitly, so a tiny wattle exposure either
  resolves coherently or fades to intact daub rather than flickering;
- keep cavity AO local and prevent dark crack texels from turning the whole
  distant infill gray.

At LOD0, damaged variants may retain finish edges, fibres, deep cavities, and a
panel-specific wattle bake or limited geometry. LOD1 should keep macro
troweling, major finish loss, and only the largest cavities. LOD2's
`FachwerkBaked` façade should normally collapse intact infill to its averaged
finish color; a rare large damage patch may remain as a low-frequency albedo
shape, but fine cracks and visible weave should disappear. Every LOD must
preserve mean panel tone and deterministic patch position to avoid popping.

## Recommended implementation sequence

### 1. Define the material and scale contract

- Split infill substrate from exterior/interior finish in the building data.
- Decide which 1544 building populations receive wattle-and-daub rather than
  masonry or another infill.
- Bind the generated maps in the isolated texture lab first, then create a
  palette/tint-aware tactical material rather than continuing to display the
  checker.
- Reconcile the recipe's 1.5 metre tile with the mesh's 2 metre repeat.

### 2. Make intact material convincing before damage

- Build a broad daub body with aggregate/fibre composition at physical scale.
- Add a separate thin lime-plaster/limewash finish variant.
- Replace crossed sine waves with bounded, overlapping hand/trowel/float
  strokes whose direction persists over plausible work areas.
- Keep intact relief modest; use the full 12 mm range only for deep spalls and
  cracks.

### 3. Build a coherent hidden substrate

- Generate vertical stakes and alternately woven rods in panel-local space.
- Use crossing order to derive height, normals, and AO.
- Vary spacing, diameter, bow, and breakage with stable per-panel seeds.
- Reveal it only through a staged depth mask: finish loss, coarse daub, then
  rare wattle exposure.

### 4. Move structural damage to panel masks

- Put frame-edge shrinkage, bulging, and large delamination in a façade-aware
  mask derived from panel boundaries.
- Keep small drying cracks in the repeatable base.
- Provide intact, maintained, weathered, and locally failed parameter presets;
  do not make every old building ruined.

### 5. Implement semantic mips and LOD composites

- Correct albedo and normal filtering and compensate roughness for unresolved
  relief.
- Preserve large damage shape while fading fibres, cracks, and weave in a
  controlled order.
- Bake the same palette and low-frequency infill statistics into LOD2
  fachwerk.

## Acceptance and regression tests

### Deterministic numeric tests

- Assert metres per tile and millimetres per texel, including the runtime UV
  transform.
- Measure stave spacing, rod diameter, fibre length, aggregate size, trowel
  span, crack width, finish thickness, and cavity area in metres.
- Prove alternating over/under weave order and corresponding crossing heights.
- Prove damage layers are nested: exposed wattle is a strict subset of missing
  daub, which is a subset of missing finish.
- Bound exposed-wattle coverage separately for maintained and ruined presets;
  the maintained preset should normally be zero.
- Test seam continuity of the intact base and continuity across triangulated
  façade runs.
- Verify linear-light albedo mips, decoded/renormalized normal mips, roughness
  compensation, and deterministic damage coverage at every mip.
- Verify stable per-panel seeds and stable LOD patch positions.

### Visual fixtures

Use neutral and grazing light on:

- one pristine limewashed panel, one intact raw-daub panel, one repaired panel,
  and one locally failed panel;
- a cutaway/reference panel that shows stakes, alternating withies, daub body,
  finish coat, and their relative depths;
- a full fachwerk bay with actual sill, posts, braces, openings, and panel
  boundaries;
- a row of many bays to reveal repeated cavities, synchronized trowel waves,
  and scale mismatches;
- a 2 by 2 intact tile plane for seams, but not as the sole weave test;
- matching exterior/interior and LOD0/LOD1/LOD2 views at transition distances;
- stationary and slow camera motion to expose fine-crack or fibre shimmer.

An independent visual reviewer should reject a result where intact panels show
a decorative basket weave, rods continue through timber, the substrate has no
over/under depth, all panels repeat one hole, trowel marks look like periodic
waves, cracks are uniform black Voronoi borders, height/noise is equally strong
at every scale, finish loss does not precede daub loss, the material scale
changes between the lab and façade, or LOD changes alter the average infill
color.

## Evidence, inference, and project decisions

- **Evidence:** German fachwerk used multiple infill traditions, including
  stakes and woven rods covered on both sides with fibre-reinforced earth;
  exterior daub was commonly smoothed and could receive compatible lime
  plaster and limewash; shrinkage and timber movement produce characteristic
  cracks and edge gaps; practitioner material workflows separate substrate,
  finish, and controllable damage layers.
- **Inference:** ordinary inhabited façades should show mostly intact finished
  infill; visible wattle should be rare localized deep damage. The affordable
  procedural solution is a tileable intact surface plus panel-local layered
  damage and substrate data, not a universal woven bitmap.
- **Repository decisions still required:** the clean infill/finish schema;
  which building populations and sides use each finish; whether deep wattle is
  baked, parallaxed, or geometric at LOD0; how palette tint combines with the
  procedural albedo; and how the far fachwerk bake consumes the same statistics.

The minimum credible milestone is a side-by-side fixture proving that intact,
weathered, and locally failed panels are visibly the same layered construction:
only the deepest localized failure reveals an organized woven substrate, all
features retain their physical scale on a real fachwerk bay, and the result
converges stably through the LOD chain.
