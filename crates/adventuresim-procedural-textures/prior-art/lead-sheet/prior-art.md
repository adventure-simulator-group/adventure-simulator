# Historic lead-sheet prior art

## Scope and historical contract

This report concerns exactly the `LeadSheet` procedural surface. Its intended
uses are lead-covered roofs and spires, gutters, valleys, flashings, weatherings,
parapet details, and other architectural work actually made from sheet lead. It
must not become a generic dark metal for iron hardware, a corrugated modern
roof, or a substitute for slate.

For a 1544 setting, “sheet” does not imply modern rolled strip. Historic England
notes that lead roofing was traditionally cast and only later became rolled
sheet. Conservation literature describes early sheet as sand-cast: molten lead
was poured over a prepared bed and struck level with a board. Sand-cast lead had
already been used extensively for medieval roofs and weatherings
([Historic England: Repair or renew an older
roof](https://historicengland.org.uk/advice/your-home/maintain-repair/roofs/),
[Building Conservation: Cast Lead
Ornament](https://www.buildingconservation.com/articles/cstlead/castlead.htm)).

This evidence is British, so it does not by itself establish how common a lead
roof was on every class of German building. It does establish the relevant
pre-industrial manufacturing family. The visual contract for this recipe should
therefore be **sand-cast and hand-worked lead**, used selectively on costly or
important architecture and on weatherproof details. Modern machine-roll marks,
perfect standing-seam grids, and factory-identical sheets are inappropriate
defaults.

## Repository facts and constraints

The following are current repository observations, not external claims.

- `lead_sheet.rs` owns a dedicated deterministic 512 by 512 surface recipe. A
  tile represents 1.6 metres square, about 3.125 mm per base texel, with a
  declared full height range of 1.4 mm.
- It emits sRGB albedo, OpenGL tangent-space normal, scalar height, and packed
  AO/roughness/metallic, with ten mip levels and repeat sampling.
- The base surface combines periodic noise at three scales with two directional
  sinusoidal “roll” bands. Patina, height, albedo, and roughness are partially
  correlated to those bands.
- Base color is a dark blue-grey family. AO is nearly white. Roughness occupies
  a broadly dull range and metallic remains high, with slight reduction as the
  generated patina increases.
- Tests establish repeatability, bounded edge discontinuity, physical scale,
  broad channel ranges, and mip completeness. They do not establish sand-cast
  morphology, semantic map correlation, linear-light color mips, normalized
  normal mips, or runtime roof appearance.
- The shared mip helper averages encoded RGBA bytes for every channel. That is
  acceptable for many scalar fields, but it neither filters sRGB color in linear
  light nor decodes, averages, and renormalizes normal vectors.
- The module documentation already assigns panel edges, standing/folded seams,
  laps, nails, gutter profiles, and edge wear to geometry or placement masks.
  The repeatable texture is intentionally the quiet material inside a sheet.
- The texture is renderer-independent. Roof-bay layout, UV generation, seam
  geometry, fastener placement, runoff direction, material blending, and
  building-level weather exposure belong to consumers.

## Historical fabrication and the forms it leaves

### Sand casting should replace the generic rolling signature

**Evidence.** A conservation account describes traditional sand casting from
Roman times onward: molten lead runs down a gently inclined table of fine,
beaten sand; the table inclination, pour speed, and craft determine thickness.
One face is rough from the sand while the smoother face can show uneven blues
and browns from oxides and impurities
([Building Conservation: Sand Cast
Leading](https://www.buildingconservation.com/articles/sand-cast-leading/sand-cast-leading.htm)).
Historic Environment Scotland similarly distinguishes medieval sand-cast sheets,
with a distinctly rough underside, from later milled sheet, which is smooth and
uniform on both faces
([Engine Shed: Roofing
Leadwork](https://www.engineshed.scot/building-advice/building-components/roofs/roofing-leadwork/)).

The early process used a board or “strickle” supported on rails to make the
thickness as even as possible. Contemporary conservation practice describes
the resulting historic appearance as less regular than milled lead, not as
deeply corrugated or randomly hammered everywhere.

**Inference for this recipe.** Remove the repeating sinusoidal roll bands from
the 1544 default. Replace them with a restrained sand-cast hierarchy:

1. broad, low-amplitude thickness drift associated with pour and striking;
2. faint directional strickle drag, intermittent rather than a continuous wave;
3. fine granular sand impression on the exposed face only when that face is
   chosen by the asset;
4. sparse shallow oxide/impurity blooms on a smoother cast face; and
5. occasional hand-dressed zones near actual folds or details, supplied by a
   placement mask rather than tiled across every square metre.

The base must remain quiet. A historic cast sheet was made deliberately flat
enough to shed water. Large repeated waves would resemble faulty modern sheet,
cloth, or stylized hammered pewter.

### Hammering, dressing, and bossing are localized construction events

**Evidence.** Early sheet could be dressed over timber or metal cores, and
raised decoration could be bossed by beating it into a die. Conservation
literature notes that hammering produced a fine-grained structural texture
rather than deep regular dents. Modern craft guidance describes lead as soft
enough to be cold-worked, dressed to roof profiles, and bossed into complicated
shapes
([CIPHE: An introduction to sheet-lead
weathings](https://www.ciphe.org.uk/globalassets/media/shop/assets/ciphe-insight-guide-sheet-lead.pdf)).

A SideFX practitioner answering how to bend sheet metal points to Vellum
plasticity rather than trying to invent bending in the material
([SideFX forum: Bending/Breaking Metal in
Houdini](https://www.sidefx.com/forum/topic/70118/)). This is a modern DCC
technique, not historical evidence, but it illustrates the correct digital
ownership: large permanent deformations belong to geometry.

**Inference for this recipe.** The texture may contain sub-millimetre hand-
working response, but roof turns, dressed tile contours, roll ends, corners,
and bossed ornaments need geometry or a detail-specific bake. A generic tile
cannot know where a plumber applied force. Use construction masks to add:

- small elongated tool facets following the local fold;
- compression/smoothing immediately beside a bossed corner;
- mild roughness change where the surface was worked;
- localized thinning only if a damage state calls for it; and
- separate ornamental embossing for explicitly decorated objects.

Do not stamp uniform circular hammer dents over every bay. Do not represent a
fold solely as a dark albedo stripe.

## Bays, seams, laps, rolls, and fasteners

### Roof layout is a system of movement-capable bays

**Evidence.** Historic England explains that sheet-metal roofs use panels and
weather-tight seams because the metal expands and contracts. Its conservation
guide distinguishes vertical joints—hollow rolls, wood-cored rolls, standing
seams, or welts—from joints across the fall, where laps or drips are used. It
also warns that over-fixing restricts movement and causes fatigue cracks or
thermal ripples
([Historic England: Practical Building Conservation,
Roofing](https://historicengland.org.uk/images-books/publications/roofing-conservation/roofing-marketing-spread/)).

The same guide describes copper clips concealed within hollow-roll joints so
that stresses are distributed without holes through the lead. A nailed
undercloak can be covered by an overcloak; visible fixing is therefore not a
regular field of exposed nails across the sheet.

**Inference for this recipe.** A roof assembler should create coherent bays
whose long direction follows the fall. It should choose historically appropriate
joint profiles and place transverse laps/drips consistently with drainage.
These are geometry, because they change silhouette, shading, occlusion, water
flow, and collision at close range. The surface texture should receive masks
for seam-adjacent polish, grime, oxide retention, and local runoff, but should
not contain a baked universal seam grid.

Fasteners should be structural and sparse. Prefer hidden clips and covered
nails where the chosen detail requires them. Exposed bright rivets scattered at
equal intervals are a modern-industrial visual trope, not a safe default for
lead roofing.

### Deformation and damage must follow construction

**Evidence.** Historic England associates restricted movement with fatigue
cracks and thermal ripples, and excessive bossing with local thinning and
premature cracking. Hollow rolls can also crack where turned down at roof
edges. These failures concentrate at restraints, turns, and joints rather than
appearing as an isotropic crackle over the sheet.

**Inference for this recipe.** If an aged/damaged variant is needed, derive it
from bay topology:

- gentle long ripples between restrained edges;
- creep or sag following gravity in gutters and vertical coverings;
- cracks at roll noses, tight turns, fixings, or overworked corners;
- compression and contact darkening along folds; and
- cleaner or polished crests where water or handling abrades deposits.

Such states need a roof-space coordinate system and placement masks. The
seamless substrate should not hallucinate damage without knowing fall, support,
or age.

## Oxidation, patina, and runoff

### Lead becomes dull grey, not orange rust or green copper

**Evidence.** CIPHE describes freshly cut lead as bright silver, ordinary lead
as bluish grey, and moist-air weathering as a slow transition to a dull grey
patina. It also notes that early patina can wash off in rain and leave grey-white
streaks on adjacent brick or tile. Conservation science identifies outdoor
lead products including lead oxides, basic lead carbonates, cerussite, and in
polluted air lead sulfites/sulfates
([Shreir's Corrosion overview: atmospheric corrosion of
lead](https://www.sciencedirect.com/topics/materials-science/underground-corrosion),
[Getty Conservation Institute: Lectures on Materials Science for Architectural
Conservation](https://www.getty.edu/conservation/publications_resources/pdf_publications/pdf/torraca.pdf)).

The clean-air carbonate/sulfate film is protective and comparatively insoluble.
Lead is therefore not iron: it should not acquire orange-red rust, deep scaling,
or broad flaking metal loss. Nor should it turn verdigris green like copper.

**Inference for this recipe.** Establish explicit weather states:

- **fresh cut/worked:** rare, locally silver-grey, lower roughness, restricted
  to cuts or very recent handling;
- **young exposed:** blue-grey with uneven darkening and early pale carbonate;
- **mature stable:** predominantly dull medium grey, subtle cool/warm variation,
  high but non-chalky roughness, intact metallic substrate response; and
- **stressed/contaminated:** local pale bloom, dark sheltered areas, or deposits
  associated with actual water and neighboring materials.

Avoid a generic colorful “patina noise.” The mature surface should read first
through reflection width and muted grey value, with color shifts subordinate.

### Water flow is directional and crosses material boundaries

**Evidence.** Conservation guidance explicitly records patina washed from new
lead as grey-white streaking on lower masonry or roof tiles. Lead roofing and
weathering are installed to channel water away from junctions, so water paths
are dictated by slope, seams, gutters, drips, and discharge points—not by
texture UV noise.

**Inference for this recipe.** The base tile may contain small-scale patina
variation, but streaks and edge accumulation require building context. Generate
a runoff mask from gravity projected into each roof face and route it around
raised seams, toward valleys/gutters, and over drips. Use it across materials:
it should be able to pale the lead near active wash paths and deposit grey-white
streaks on the slate, plaster, brick, or stone below. Sheltered undersides and
seam troughs can retain darker deposits. Never tile identical vertical streaks
across a roof regardless of pitch.

## Procedural-material practice

### Begin with structure and reuse masks across correlated outputs

**Evidence.** At GDC 2018, Daniel Thiger presented a production methodology for
photorealistic procedural materials in Substance Designer, emphasizing reusable
workflows and efficient variation
([GDC Vault: Creating Photorealistic Procedural Materials with Substance
Designer](https://www.gdcvault.com/play/1024844/Creating-Photorealistic-Procedural-Materials-with)).
Environment Material Artist Stan Brown's tarnished-metal breakdown starts from
height structure, then reuses scratches, grunge, and height-derived masks in
albedo and roughness. He stresses checking the material in the target engine,
because an attractive shiny authoring-tool preview can be physically implausible
in context
([80 Level: Creating Tarnished Metal with Ornament in Substance
Designer](https://80.lv/articles/creating-tarnished-metal-with-ornament-in-substance-designer)).

SideFX hard-surface artists similarly generate UV seams and alignment from
procedural topology rather than manually guessing per output; forum discussion
notes that clean planar/orthogonal strips often require explicit procedural
alignment constraints
([SideFX forum: Procedural Approach to UV
Alignment](https://www.sidefx.com/forum/topic/61107/)).

**Inference for this recipe.** Maintain named intermediate fields:

1. casting thickness drift;
2. exposed-face sand grain;
3. strickle direction and intensity;
4. hand-work mask;
5. patina maturity;
6. wetting/runoff exposure;
7. seam/fold proximity; and
8. damage or fresh-metal exposure.

Derive height, normal, albedo, roughness, metallic, and any AO from the same
fields. This prevents the common failure in which color stains, roughness
patches, and dents are all unrelated noise layers.

### PBR channels should describe the same physical state

**Evidence.** A GDC 2016 end-to-end PBR guide requires base color without baked
lighting, high conductor reflectance, primarily binary metalness, and treats
roughness as the main authored microsurface control
([GDC 2016: An End-to-End Approach to Physically Based
Rendering](https://media.gdcvault.com/gdc2016/Presentations/Bugden_Sam_AnEndTo.pdf)).
Production tarnished-metal breakdowns likewise build roughness deliberately
rather than copying height, and repeatedly validate inside the engine under
multiple lights.

**Inference for this recipe.** Correlate channels by state:

- cast height drift changes normals but should barely change albedo;
- sand grain primarily broadens highlights and adds minute normal response;
- stable carbonate patina raises roughness, lightens/desaturates base response,
  and may reduce effective metallicity only insofar as a real non-metallic film
  covers the conductor;
- fresh cuts lower roughness and expose stronger metallic response;
- sheltered dirt increases roughness and changes albedo but is non-metallic;
- contact polish lowers roughness at fold crests without turning them white;
- AO belongs only to real micro-cavities or geometry contact, not broad painted
  shadows.

The current high-but-variable metallic values are directionally plausible for a
thin surface film over lead, but arbitrary greys in a metalness channel can also
mean an undefined blend. Decide whether patina is represented as coverage of a
dielectric film or as a homogeneous stylized conductor, document that contract,
and test it under real environment reflections.

## Scale, UVs, tiling, and mips

### Use sheet-local planar coordinates aligned to construction

**Evidence.** Lead roofs are organized by bays, vertical joints, and transverse
laps/drips, all relative to the fall. Procedural hard-surface UV practice favors
stable seam groups and aligned islands when the material has a meaningful
direction. This material does not need triplanar projection on the ordinary
planar faces for which it is intended.

**Inference for this recipe.** Generate UVs per physical sheet or continuous
bay. Align V downslope and U across the bay. Preserve real-world texel density
through roofs, gutters, valleys, and flashings; straighten long islands where
that does not distort a formed corner. At tight folds, either unfold the strip
continuously or use a dedicated formed-detail mesh/bake. Do not reset UV phase
at every triangle, world-project through both sides of thin lead, or rotate the
casting/working direction randomly from panel to panel.

The base 1.6 metre tile is useful as a quiet substrate scale, but it should not
define bay width. Roof layout owns actual sheet dimensions and seams. Add a
low-frequency, non-repeating per-sheet parameter or material instance so a
large roof does not expose an obvious 1.6 metre square noise stamp.

### Mips must preserve energy and normal direction

**Evidence.** GDC PBR guidance treats base color, roughness, metallic, and
normal as distinct material properties. They therefore should not all be
filtered as interchangeable bytes. Metal surfaces are especially sensitive to
microsurface filtering because distant highlight width carries much of their
identity.

**Inference for this recipe.** Generate semantic mip chains:

- filter albedo in linear light, then re-encode sRGB;
- decode, average, and renormalize tangent-space normals;
- average height as a scalar while separately retaining any variance needed to
  increase distant roughness;
- filter roughness in a variance-aware manner if practical, so lost normal-map
  detail does not make the distant roof unnaturally polished;
- keep metallic/patina coverage physically meaningful rather than averaging
  unrelated labels; and
- use anisotropic filtering for long shallow roof views.

Inspect the full mip chain on a pitched roof under moving sun and sky
reflections. Flat PNG reductions do not reveal specular crawling, normal-length
loss, or incorrect gamma filtering.

## Recommended representation

1. **Rename the visual basis from rolled to sand-cast.** Preserve the quiet
   blue-grey material but replace repeating roll waves with subtle cast
   thickness drift, optional sand grain, and intermittent strickle traces.
2. **Expose the cast face as a controlled variant.** A smoother oxide-marked
   face and a granular sand face should share material chemistry but differ in
   roughness/normal response.
3. **Keep construction in geometry.** Generate bays, hollow/folded rolls,
   standing seams where historically appropriate, laps, drips, gutters,
   flashings, and bossed corners from roof topology.
4. **Use hidden or covered fixing by default.** Place clips/nails only according
   to the selected joint detail; avoid a decorative rivet grid.
5. **Add placement-aware masks.** Seam proximity, fold working, contact polish,
   runoff, sheltered deposits, and damage must follow actual geometry and
   gravity.
6. **Represent mature lead as restrained grey.** Reserve silver for fresh cuts,
   pale carbonate for young wash/bloom, and dark deposits for sheltered zones.
   Exclude iron rust and copper verdigris.
7. **Correlate every channel.** Casting, patina, dirt, fresh exposure, and hand
   work must have explicit, consistent effects on albedo, roughness, metallic,
   normal, and height.
8. **Use sheet-local UVs.** Align the material with fall and preserve metre
   scale through formed details. Break repetition per sheet without changing
   texel density.
9. **Build semantic mips.** Linear-light color, renormalized normals, and
   roughness compensation are higher-value than adding more base-level noise.

## Deterministic tests and visual acceptance

### Base substrate

- Generation is byte-identical for a given recipe/variant.
- The tile is seamless in value and first derivative within declared tolerance.
- A two-by-two preview has no obvious periodic waves, square clouds, or mirrored
  quadrants.
- The 1.6 metre scale and 1.4 mm height range remain explicit; measured RMS and
  peak slope stay shallow enough for intentionally flat roofing sheet.
- Sand-face grain, smooth-face oxide blooms, and strickle traces occupy declared
  non-overlapping frequency/amplitude ranges.
- No base-tile feature resembles a standing seam, lap, nail, crack, or runoff
  streak; those require contextual masks.

### Channel coherence

- Every map has a complete mip chain generated according to its semantic type.
- Albedo mips are linear-light correct and contain no baked directional light.
- Every normal vector is finite and normalized at every mip.
- Normal gradients agree with height gradients under the engine's OpenGL
  convention.
- Roughness, metallic, and albedo changes for fresh lead, mature patina, dirt,
  and polish preserve the declared ordering.
- AO remains near unoccluded over open sheet and darkens only at justified
  cavities/contact masks.
- A diagnostic correlation report proves that patina and hand-work fields affect
  the intended channels together rather than as independent noise.

### Roof assembly

- Bay seams run with fall; transverse joints use the selected lap/drip detail.
- Seams and rolls have silhouette and shadow at close distance and simplify to
  stable baked/normal representations only at an explicit LOD transition.
- Fasteners match the joint recipe, remain covered where intended, and never
  form an unexplained exposed grid.
- UV scale and orientation remain consistent across flat roofs, pitched roofs,
  valleys, gutters, and flashings. No fold stretches the texture beyond a
  declared tolerance.
- Runoff paths respond to roof pitch and topology and can continue as grey-white
  staining onto materials below.

### Runtime visual evidence

- Capture fresh, young, and mature states under overcast sky, low grazing sun,
  and moving environment reflections.
- Include a flat bay, steep roof, valley, gutter, flashing, hollow/folded seam,
  bossed corner, and distant roof cluster.
- Capture consecutive frames during camera translation and sun/reflection
  movement through mip transitions.
- Fail on bright chrome, charcoal paint, orange rust, green verdigris, deep
  hammered dimples, identical tiled streaks, swimming UVs, or specular sparkle.
- Compare cast and smooth faces at identical light/exposure; difference should
  arise mainly through micro-normal and roughness response, not unrelated color.
- Review a full roof, not only a material sphere. The material's success depends
  on bay rhythm, joints, drainage, and scale.

## Pitfalls to avoid

- Modern rolled or corrugated sheet on a 1544 roof.
- Continuous sinusoidal “rolling” waves repeated every tile.
- Treating sand-cast texture as coarse stucco or hammered pewter.
- Baking seams, laps, nails, and runoff into one universal square tile.
- Evenly scattered exposed rivets.
- Orange iron corrosion or green copper patina.
- Colorful grunge without a chemical, water-flow, or fabrication cause.
- Copying height directly into roughness or AO.
- Using arbitrary mid-grey metalness without a stated patina-film model.
- Baked highlights or shadows in albedo.
- World-aligned streaks that ignore roof fall and formed-sheet UV continuity.
- Ordinary byte-averaged sRGB and normal mips.
- Judging a sphere or still PNG without roof geometry and moving reflections.

## Bottom line

The current recipe already has the right architectural boundary: it is a quiet
substrate, while seams, laps, profiles, fasteners, edges, and runoff belong to
geometry or placement context. Its main historical error is the repeated
rolling signature. A 1544 lead surface should derive from sand casting and
hand working: slight thickness drift, optional granular cast-face character,
intermittent strickle traces, subdued blue-grey oxidation, and localized
dressing around real construction details.

The end product should look expensive, heavy, malleable, and durable—not busy.
At close range, restrained cast irregularity and hand-worked folds broaden and
break highlights. Across a roof, coherent bays and drainage details provide the
large-scale read. At distance, semantic mips must preserve a stable dull-metal
reflection without shimmer. The strongest next iteration is therefore to
replace roll waves with a documented sand-cast model, add sheet/roof contextual
masks, and validate the entire assembly under moving reflected light.
