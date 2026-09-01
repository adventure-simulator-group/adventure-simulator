# Dressed stone procedural texture prior art

## Scope

This report concerns exactly the `DressedStone` procedural surface for regular
cut masonry and architectural dressings: ashlar wall leaves, quoins, jambs,
sills, lintels, voussoirs, mullions, stringcourses, copings, parapet crowns,
stairs, and other worked castle or church stone. It covers coursing and unit
layout, joint scale/profile, hand tooling, edge wear, lithology, damp and soot,
causal PBR channels, physical scale, UVs, tiling, mips, LODs, and integration
with rubble masonry.

It does not assume every worked stone face should look like one gray rectangular
tile. “Dressed stone” describes the preparation and fit of units; lithology,
tool, finish, course size, joint width, and architectural role vary. Fine
ashlar, roughly squared facing, drafted-margin work, quarry-faced/rusticated
blocks, delicate tracery, and coping stone need related but distinct presets.

## Repository facts and constraints

The following are facts observed in this worktree, not claims from external
sources.

- `DressedStone` produces albedo, OpenGL normal, height, and ARM maps. Its 1024
  by 1024 tile represents 7.2 metres square, about 7.031 mm per source texel,
  with a declared 24 mm full height range.
- The tile has 22 courses with 11–15 blocks per course. Course heights are
  tested at 0.27–0.37 metres and block widths at 0.32–0.90 metres.
- Courses receive a near-half alternating offset plus small deterministic
  variation. Units are near-rectangular with very slight wavy edges, 8–13 mm
  head joints, and 10–14 mm bed joints.
- Stone faces receive per-unit depth, planar tilt, broad waviness, occasional
  localized edge wear, and on about 44 percent of blocks two to five curved,
  interrupted groove marks at a per-block angle.
- The normalized face-to-mortar difference is about 0.53, corresponding to
  roughly 12.7 mm of the declared relief. Mortar is therefore strongly recessed
  for a material described as regular ashlar.
- The six stone albedos are narrowly clustered neutral beige-gray. Face color
  is shifted from surface height, which risks encoding form/shadow into albedo.
  The generator has no named lithology, bedding plane, quarry batch, cut face,
  fresh interior, or finish family.
- Roughness is very high and increases with a random mineral scalar, proximity
  to joints, and tool-groove strength. Mortar is also extremely rough.
  Metalness is zero. AO uses multi-distance cardinal height comparisons.
- Tests prove determinism, exact analytic periodicity, the current course/block
  ranges, recessed joints and near-planar faces, sRGB albedo,
  channel variation/nonmetalness, and complete mip dimensions. An ignored
  deterministic visual-review export already creates maps, tiled views, and
  distance reductions.
- The shared mip helper averages encoded bytes. It does not average sRGB
  albedo in linear light, decode/filter/renormalize normals, carry unresolved
  normal variance into roughness, or preserve thin-joint coverage.
- Building wall UVs repeat every 2 metres, whereas `DressedStone` declares a
  7.2 metre tile. Binding without a UV transform would make every course and
  block about 72 percent smaller than intended.
- `DressedStone` exists beside `RubbleMasonry`, but tactical presentation maps
  all `FortifiedMasonry`, other wall stone, and `CrownMasonry` to one generic
  checker. The procedural recipes are not assigned by architectural role.
- Opening jambs, heads, sills, stone lintels, pointed voussoirs, mullions,
  crowns, towers, and fortification elements exist in resolved building data,
  but many converge on broad wall material classes. Their UVs are commonly
  planar or cuboid projections with the same generic 2 metre repeat; they do
  not carry unit-local bedding/tool orientation or a specific stone finish.

The first requirement is therefore not extra stone noise. It is a clean
material-role and physical-coordinate contract that distinguishes field wall,
dressed boundary, shaped opening stone, and crown/coping.

## Construction and craft evidence

### Ashlar quality is expressed through fit, course, and finish

Historic England's archaeological recording manual treats masonry surface
treatment and stonework coursing as reportable, distinguishable evidence rather
than one generic category
([Historic England, *Archaeological Recording Manual*](https://historicengland.org.uk/content/docs/research/historic-england-archaeological-recording-manual-2018)).
Historic conservation guidance likewise asks practitioners to match the method
of construction, coursing, and dressing—such as ashlar, rock-faced,
punch-dressed, herringbone tooling, or margins
([Staffordshire County Council, *Historic Structures and Areas: Practical Conservation and Design*](https://www.staffordshire.gov.uk/Environment/Environment-and-Countryside/HistoricEnvironment/Documents/Historic-Structures-and-Areas-Practical-Conservation-and-Design5.pdf)).

Ashlar is dimension stone cut accurately on the bedding/joint faces so units
can form regular work with close joints; the exposed face may be smooth,
tooled, quarry-faced, drafted, or otherwise deliberately finished. Historic
Environment Scotland's ashlar guide exists specifically because its fine
joints and masonry character demand different repointing treatment from rubble
([HES, *Repointing Ashlar Masonry*](https://www.historicenvironment.scot/publications/all/publication/?publicationid=5073eef0-ee70-4398-8f86-a5950102f1b2)).
A conservation study gives a typical fine ashlar joint around 3 mm and records
horizontal tooling/broaching as a recognizable surface treatment
([Kayan, *Green Maintenance for Historic Masonry Buildings*](https://www.ros.hw.ac.uk/server/api/core/bitstreams/fcc43748-28f2-4a5e-890d-91e1e69741f9/content)).

**Inference for `DressedStone`:** the current 8–14 mm joints and approximately
13 mm recess suit roughly squared or deliberately recessed work better than
fine ashlar. Split the recipe into named finish presets:

- fine ashlar: tighter, near-flush joints and very planar faces;
- ordinary dressed/coursed facing: modest joints and restrained variation;
- drafted-margin or quarry/rock-faced block: true border-and-centre structure
  with greater relief;
- architectural dressings: role-specific unit layout and finish, often finer
  than the field wall.

The existing course and block ranges are plausible as one large wall-facing
family, but openings, mullions, voussoirs, coping, and quoins should not inherit
that rectangular course grid as a texture illusion.

### Tool marks are organized evidence of manufacture

Medieval masons' marks and tooling are decisive cuts made by skilled workers;
some later masons smoothed tooling away entirely
([University of Warwick, *Medieval Masons’ Marks*](https://warwick.ac.uk/fac/arts/scapvc/arthistory/people/ja/research/masonsmarks/)).
Archaeological recording guidance treats different surface/tool treatments as
diagnostic evidence. Fine surfaces could be carefully pared; other finishes
retain broach, punch, axe, drag, herringbone, or margin patterns. Mason's marks
are a separate sparse semantic category, not ordinary face roughness
([Kent Archaeological Society, *Rochester Cathedral Masons’ Marks*](https://www.kentarchaeology.org.uk/journal/141/rochester-cathedral-masons-marks)).

**Inference:** the present random curved grooves are too weakly tied to a
specific tool or finish. For each block, select one finish system and construct
coherent strokes:

- broad axe/pick reduction beneath finer finishing where visible;
- parallel broach/tooth-chisel grooves with stable spacing and direction;
- punch/point texture with controlled impact spacing;
- drafted margins as a flat worked border surrounding a rougher centre;
- sparse, separately generated mason's marks only where reference and visual
  scale justify them.

Tool direction should persist across a worked face or work patch and respect
block-local coordinates. It should not rotate independently per texel, cross
joints, or automatically make albedo darker. At 7.031 mm per texel, only
centimetre-scale tooling is robust in height. Fine tooth marks should be
restrained normal/roughness detail and fade before shimmering.

### Lithology controls workability, bedding, and weathering

Historic England requires replacement stone to match chemical, physical, and
mineralogical properties, not color alone
([Historic England, *Identifying and Sourcing Stone for Repair*](https://historicengland.org.uk/advice/technical-advice/buildings/building-materials-for-historic-buildings/identifying-and-sourcing-stone-for-repair/)).
Building stones commonly reflect local geology, giving settlements regional
distinctiveness. Sedimentary structures, grain size, mineralogy, and known
failure/weathering behavior are part of that identification.

Natural bedding also matters in construction. The Palace of Westminster's
stone history records decay associated with blocks laid without maintaining
their natural bedding plane as well as atmospheric pollution
([UK Parliament, *The stonework*](https://www.parliament.uk/about/living-heritage/building/palace/architecture/palacestructure/the-stonework/)).
This is nineteenth-century evidence, not a claim about one 1544 German castle,
but the material principle transfers to bedded sedimentary stone.

**Inference:** build lithology presets shared with `RubbleMasonry` but expressed
through cut faces:

- limestone: region-specific warm/cool family, pores/fossils only at resolvable
  scale, potential solution and granular decay;
- sandstone: bedding and grain orientation, iron/mineral staining where
  appropriate, granular erosion;
- granite/gneiss: crystalline mineral family, harder sharper tooling response,
  lower porosity, different fracture;
- slate/metamorphic stone: cleavage-oriented units and edges.

Each building/quarry batch should select a dominant lithology with bounded
unit variation. A block's color, grain, bedding, tool response, roughness, and
weathering must agree. Do not obtain “variety” by independently mixing unrelated
gray, beige, and brown rocks. Block-local coordinates need a bedding direction,
especially for sills, copings, lintels, and vertically oriented pieces.

### Dressed stone is concentrated at architectural work

Castle construction frequently combines rubble and ashlar/dressed masonry.
Archaeological evidence records larger dressed stones set first as quoins and
around openings, with intervening wall fabric filled afterward
([Council for British Archaeology, *The Anglo-Saxon Church: Papers on History, Architecture, and Archaeology*](https://archaeologydataservice.ac.uk/catalogue/adsdata/arch-281-1/dissemination/pdf/cba_rr_060.pdf)).
A castle-construction guide similarly distinguishes rubble and regularly cut,
more neatly jointed ashlar
([Herefordshire Through Time, *Preparation and construction*](https://htt.herefordshire.gov.uk/herefordshires-past/the-medieval-period/castles/building-a-castle/preparation-and-construction/)).
These British examples establish a widely useful construction distinction, not
a universal German stylistic recipe.

**Inference for the building generator:** use `DressedStone` where geometry
already expresses skilled cut work:

- quoins that turn and bond the corner;
- jamb, sill, lintel, arch/voussoir, mullion, and tracery units;
- stringcourses, plinths, corbels, stair treads, copings, and crown caps;
- selected ashlar wall leaves or prestigious entire façades.

Use `RubbleMasonry` for the appropriate field, and a distinct core/hearting
material for breaks. A texture grid must not continue behind a voussoir fan,
around a quoin, or across a mullion. The shaped stone geometry owns the unit;
`DressedStone` supplies lithology and intra-unit finish.

SideFX practitioners reach the same production conclusion for corners and
arches: low-poly unit geometry plus texture detail avoids implausible flat
tiling and preserves thickness
([SideFX forum, *Stone wall generation*](https://www.sidefx.com/forum/topic/53335/)).

## Procedural-art evidence and transferable workflows

A modular castle production breakdown uses a single parameterized Substance
stone graph with input masks for variants, then adds world-aligned material
variation because an unmodified 2 × 2 metre tile was visibly repetitive
([80 Level, *Medieval Castle Production: Working with Modular Packs*](https://80.lv/articles/001agt-medieval-castle-production-working-with-modular-packs/)).
A more recent Gothic environment uses shared trim sheets for arches, pillars,
cornices, and architectural details, while packed masks separately control edge
highlights, material blending, cavity dirt, and world-position moss
([80 Level, *Modular Gothic Environment with Procedural Systems and Custom Shaders*](https://80.lv/articles/making-of-a-modular-gothic-environment-with-procedural-systems-trim-sheets-custom-shaders)).

**Inference:** keep one causal lithology/tool generator, but expose masks and
parameters for face finish and role. Near architectural pieces can use a trim
sheet or unit-local atlas so tooling and bedding follow the shaped stone. Large
wall fields can use physically scaled tiling plus stable world/building masks.
Do not duplicate unrelated bespoke stone materials for every castle piece.

GDC material-layer practice supports this division: reusable base materials,
damage, and environmental effects remain separately controllable
([Pettineo, *Crafting a Next-Gen Material Pipeline for The Order: 1886*, GDC 2014](https://media.gdcvault.com/GDC2014/Presentations/Pettineo_Matt_Crafting_A_Next-Gen.pdf)).

## Mortar, joints, and edge wear

SPAB guidance emphasizes localized rather than indiscriminate repointing and
distinguishes fine lime pointing for ashlar
([SPAB, *Repointing*](https://www.spab.org.uk/advice/repointing)).
Joint mortar should match the chosen construction and remain compatible with
the stone. For fine ashlar, mortar may be nearly flush and visually subordinate;
rougher dressed work can tolerate wider or more recessed joints. Aggregate,
color, tooling, shrinkage, and repair age belong to the mortar, not the nearest
stone's random scalar.

Edge wear should also be causal:

- sheltered wall-face arrises can remain sharp for long periods;
- sills, stairs, thresholds, parapet/coping tops, and hand-contact zones receive
  role-specific mechanical wear;
- water-exposed horizontal surfaces weather differently from vertical faces;
- frost and salt spalling depend on porosity, bedding, water, and exposure;
- impact chips reveal lithologically consistent interiors.

The current sparse localized edge-wear field is preferable to uniformly
rounding every block, but it lacks role and orientation. Preserve the sparse
principle while moving strong wear to object/building-space masks.

## Soot, damp, salts, and biological growth

Historic masonry response depends on pore structure and geology. Flooding and
moisture guidance describes efflorescence, sub-surface salt crystallization,
spalling, freeze/thaw, and different vulnerability among stone families
([Historic England, *Masonry Buildings*](https://historicengland.org.uk/advice/technical-advice/flooding-and-historic-buildings/building-construction/masonry-buildings/)).
Building-stone research identifies black crusts containing gypsum and soot as a
specific pollution-related deterioration rather than generic dark noise
([Gomez-Heras et al., *A Geological Perspective on Building Stone Deterioration*](https://doi.org/10.3390/ATMOS11080788)).

**Inference:** keep the base tile relatively clean and apply deterministic
building-space effects:

- rising damp and salt blooms near ground/moisture paths;
- runoff below copings, sills, gargoyles, gutters, and damaged roofs;
- soot around chimneys, firing openings, urban combustion, and sheltered
  recesses;
- biological growth on persistently damp, shaded surfaces and open joints;
- cleaner wear on contact edges and exposed rain-washed projections;
- bounded repair patches with distinguishable but compatible stone/mortar.

An inhabited 1544 castle is not automatically a nineteenth-century polluted
ruin. Soot amount must follow the game's actual combustion and maintenance
context.

## Channel relationships

Stone and lime mortar are dielectric, so metalness remains zero. Channels
should share material causes without becoming copies.

- Unit layout and joint profile own the dominant height/normal/AO structure.
- Lithology owns albedo family, grain/bedding, intrinsic roughness, fracture,
  and weathering response together.
- Tooling changes height/normals and may change roughness; it should affect
  albedo only through real dust, mineral exposure, or weathering—not by baking
  a dark groove.
- Planar tilt changes height/normals, not color. The present height-derived
  face-color shift should be removed or justified as a separate material cause.
- A chip lowers height, exposes a coherent interior color/roughness, and gains
  local AO only at the recess.
- Mortar has independent binder/aggregate albedo and roughness, with height set
  by the selected joint/pointing profile.
- Damp darkens albedo and usually lowers apparent roughness; salts/light crusts
  raise albedo and can roughen; soot darkens without being a height trench.

Reserve the 24 mm relief range for rock-faced/drafted presets, deep joints, and
meaningful damage. Fine ashlar faces/tooling should occupy a much smaller range.
Avoid pillow inflation and permanent black AO lines.

## UV and unit-coordinate contract

Large flat ashlar fields can use U as accumulated façade-run distance and V as
physical height. Runtime must apply the recipe's 7.2 metre scale. Continuous
wall runs should preserve course elevation and bond rather than restarting at
mesh chunks.

Architectural pieces need unit-local mapping:

1. identify the stone unit and its exposed, bed, joint, and end faces;
2. carry a local bedding direction and tool-finish orientation;
3. map coursing around real corners rather than projecting through them;
4. use arc/radial coordinates for voussoirs so unit/tool patterns follow the
   arch, not world-horizontal courses;
5. map sills/copings/treads so horizontal top surfaces receive the correct
   bedding and exposure treatment;
6. retain physical texel density independent of mesh size.

For round towers, use arc length for field-wall U. Quoins and openings should
interrupt that run with explicit shaped units. Triplanar projection is not
required for regular generated geometry, but a trim/unit atlas or member-local
UV schema is.

## Tiling, mips, distance, and LODs

A 7.2 metre tile is large, yet 22 repeated courses and fixed tool/wear blocks
can still reveal the square. Extend walls with deterministic compatible course
variants or façade unit seeds while preserving course/bond continuity. Apply
weathering in building/world space so it does not repeat with the stone tile.

Semantic mip generation should:

- downsample albedo in linear light and re-encode to sRGB;
- decode, filter, and renormalize normal vectors;
- transfer unresolved tooling/normal variance into roughness;
- preserve joint and stone coverage without shimmering thin joints;
- filter height without making one deep chip depress a whole distant block;
- reduce AO as joints/tool grooves become subpixel rather than turning the wall
  into a dark grid.

LOD0 should retain explicit shaped openings/quoins, selected tool finish, major
edge wear, and correct joint profiles. LOD1 should preserve courses, dressed
boundaries, major bedding/color, and broad relief but omit fine tooling. LOD2
should retain architectural material segmentation and stable average lithology
tone; fine joints and marks should merge. Silhouette and geometry still own
voussoirs, tracery, copings, crenellation, and large damage. LOD transitions
must not change course scale or turn dressed openings into rubble/checker stone.

## Recommended implementation sequence

### 1. Define stone-role and finish presets

- Separate fine ashlar, ordinary dressed facing, drafted/rock-faced blocks,
  and architectural trim.
- Assign rubble, dressed field, quoin, jamb/sill/lintel, voussoir, mullion,
  coping/crown, and core roles explicitly in the clean schema.
- Reconcile the 7.2 metre texture scale with generic 2 metre UVs.

### 2. Establish shared lithology parameters

- Share dominant quarry/lithology selection with `RubbleMasonry` while giving
  cut stone a face/bedding/tool response appropriate to that rock.
- Add block-local bedding and stable quarry/batch variation.
- Remove height-derived albedo shadowing.

### 3. Rebuild finish and joint causally

- Generate explicit broach, punch, drag, drafted-margin, smooth, or rock-face
  systems rather than random generic grooves.
- Give mortar independent aggregate/color and a preset-specific near-flush or
  recessed profile.
- Use finish-specific relief budgets; keep fine ashlar restrained.

### 4. Integrate architectural geometry

- Give shaped units local UV/bedding/tool axes and preserve real voussoir,
  quoin, sill, coping, and mullion boundaries.
- Bind the procedural texture in the lab and tactical scene instead of the
  generic checker.
- Ensure LOD material segmentation preserves dressed stone against rubble.

### 5. Add role/exposure weathering and semantic mips

- Drive edge wear, runoff, damp, salts, soot, and growth from geometry,
  lithology, use, and maintenance.
- Correct albedo/normal filtering, compensate roughness, and stabilize joints
  and tooling across distance.

## Acceptance and regression tests

### Deterministic numeric tests

- Assert block/course dimensions, joint widths, mortar recess, face flatness,
  tool spacing/depth, and edge-wear radius in metres for each finish preset.
- Require fine-ashlar joints and relief to be distinctly tighter than ordinary
  dressed and rock-faced presets.
- Prove course continuity and no stacked vertical joints across façade-run
  seams.
- Verify unit-local UV scale, bedding direction, and tool orientation on every
  face class.
- Verify voussoir radial layout, quoin alternation, and correct termination at
  jamb/sill/coping geometry.
- Prove lithology changes albedo, grain, tooling response, roughness, and
  weathering coherently.
- Prove mortar fields are independent of stone height/mineral IDs.
- Verify zero metalness and remove correlation between planar face height and
  albedo absent a material cause.
- Compare linear-light albedo and decoded-vector normal mip references; test
  joint coverage and deterministic LOD stability.

### Visual fixtures

Review under neutral, overcast, wet, and grazing light:

- fine ashlar, ordinary dressed, drafted-margin, and rock-faced blocks side by
  side at annotated real scale;
- isolated broach, punch, drag, smooth, and sparse mason-mark examples;
- a rubble wall with dressed quoins, jamb/sill/lintel, pointed voussoirs,
  mullion, stringcourse, coping, and crenellated crown;
- a round tower, right-angle corner, stair/threshold, and broken wall revealing
  face versus core;
- multiple lithologies with bedding direction visibly checked;
- isolated runoff, rising damp, salt, soot, biological, contact-wear, and repair
  masks;
- a long repeated wall plus matching LOD0/LOD1/LOD2 transitions under slow
  camera motion.

An independent visual reviewer should reject identical finish on every stone,
fine ashlar with cavernous black joints, random worm-like tool grooves, pillow
faces, height-baked albedo shadows, bedding that rotates arbitrarily, course
textures crossing quoins/voussoirs/mullions, stretched tower mapping, uniformly
rounded edges, tiled soot/damp, shimmering joints, or LODs that erase dressed
architectural boundaries.

## Evidence, inference, and project decisions

- **Evidence:** ashlar/dressed masonry is defined by worked, regularly fitted
  units but includes multiple face finishes; coursing, joint width/profile, and
  tooling are historically diagnostic; dressed stones concentrate at quoins,
  openings, and other architectural work; lithology and natural bedding affect
  workability and weathering; damp, salts, frost, pollution, and soot have
  distinct spatial and material causes.
- **Inference:** the robust affordable system is a shared lithology generator
  plus explicit dressed-stone finish/role presets, unit-local coordinates,
  independent mortar, and building-space weathering. The current regular tile
  is a useful ordinary-dressed baseline but its wide deeply recessed joints,
  random grooves, and overloaded castle assignment do not constitute universal
  ashlar.
- **Repository decisions still required:** German regional quarry/lithology
  inputs; finish distributions by building status/date; exact role-to-material
  schema; use of trim sheets versus generated unit UVs; tactical binding and
  palette mapping; and how LOD2 preserves dressed/rubble segmentation.

The minimum credible milestone is a real-scale castle fixture where a rubble
field terminates cleanly at lithologically matched dressed quoins and openings,
fine ashlar and rock-faced presets have visibly different joint/relief budgets,
tool marks follow explicit craft patterns and unit axes, and every architectural
boundary and average tone remains stable through the LOD chain.
