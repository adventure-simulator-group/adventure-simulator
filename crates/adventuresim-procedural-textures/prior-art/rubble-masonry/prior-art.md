# Rubble masonry procedural texture prior art

## Scope

This report concerns exactly the `RubbleMasonry` procedural surface for
vernacular stone walls, foundations, fortifications, and castle wall fields.
It covers packed irregular construction under gravity, coursing and bearing,
non-overlap and void filling, mortar and pinnings, local lithology and coherent
color, weathering, causal PBR channels, physical scale, façade UVs, repetition,
mips, LODs, and the boundary between rubble and dressed stone.

It does not prescribe one stone palette for all of Germany in 1544. Building
stone is intensely regional, and “rubble” describes the degree of working and
laying rather than a lithology. Limestone, sandstone, volcanic rock, granite,
slatey stone, river cobbles, and mixed local material require different shapes,
colors, bedding behavior, fracture, and weathering. The result should be a
family selected from world geology and building role, not generic gray rocks.

## Repository facts and constraints

The following are facts observed in this worktree, not claims from external
sources.

- `RubbleMasonry` produces albedo, OpenGL normal, height, and ARM maps. Its 1024
  by 1024 tile represents 4.8 metres square, about 4.688 mm per source texel,
  with a declared 32 mm full height range.
- The tile contains 22 variable-height horizontal rows. Each row is partitioned
  into 15–21 variable-width stone intervals, offset independently. Stones are
  approximately 0.06–0.55 metres wide and rows 0.12–0.30 metres high.
- Stone outlines are near-rectangular with sinusoidally wobbling edges and
  diagonal corner cuts. A small fraction extend upward to interlock into the
  next row. This is structured rough coursing, not uncoursed random rubble.
- Each row partitions all horizontal space before mortar-width erosion. The
  algorithm therefore avoids large lateral voids by construction, but it does
  not perform a two-dimensional packing or stability solve. It does not prove
  centre-of-mass support, avoid all vertical-joint stacking, insert pinnings,
  or model an actual three-dimensional stone depth/core.
- A test checks that most broad stones sample at least two support identities in
  the lift below. That is useful evidence of stagger, but not proof that the
  stone's bearing surface is continuous or mechanically stable.
- Every stone receives deterministic ID-based width, edge shape, planar tilt,
  face relief, depth, palette entry, and roughness. Mortar has variable 6–16 mm
  nominal joints, a periodic sine height, and an ID-derived offset.
- The eight-color palette mixes warm beige/brown stones and cool gray stones
  at near-equal unit frequency. It does not represent one explicit lithology,
  quarry/batch, bedding plane, mineral structure, or coherent mixed deposit.
- Mortar is pale and deeply recessed. Its color and height partly use the
  selected stone ID rather than independent aggregate, application, and
  weathering fields.
- Metalness is zero. Stone and mortar roughness are both very high. AO is
  derived from multi-distance cardinal height comparisons.
- Tests prove determinism, exact analytic periodicity, current size variety,
  the broad-stone multiple-support heuristic, recessed joints, non-pillow-like
  face interiors, channel variation/nonmetalness, and complete mip dimensions.
  An ignored deterministic visual-review export already produces full maps,
  tiled maps, and distance reductions for review.
- The shared mip helper averages encoded bytes. It does not filter sRGB albedo
  in linear light, decode/filter/renormalize normals, transfer unresolved
  normal variance into roughness, or preserve thin joint/pinning coverage.
- Building wall UVs repeat every 2 metres, while this recipe declares a 4.8
  metre tile. Binding it without a UV transform would make all stones 58
  percent too small.
- `WallStyle::Stone`, many default wall roles, fortifications, crowns, round
  towers, mullions, and other stone details converge on broad
  `FortifiedMasonry`, `CrownMasonry`, or generic stone presentation. The audit
  requires fortified walls to be at least 1.20 metres thick.
- The repository separately implements `DressedStone`, but tactical buildings
  currently display a generic palette checker for stone. Neither procedural
  recipe is yet assigned by wall field, quoin, opening, crown, or architectural
  trim role.

Thus the material problem and the construction problem are coupled. A good
rubble surface needs a lithology/pattern preset, and a castle needs explicit
transitions among rubble facing, rubble core, render, and dressed structural
stone.

## Construction and conservation evidence

### Rubble is irregular, but not causeless randomness

Historic England's stone conservation guide distinguishes several rubble
traditions. Hard stones that break into random shapes can form closely jointed
polygonal or rag walling; more regular material can be roughly squared and
coursed; humbler rubble walls were sometimes minimally dressed, flush-pointed,
or lime-rendered for protection
([Historic England, *Practical Building Conservation: Stone*](https://historicengland.org.uk/images-books/publications/stone-conservation/stone-marketing-spreads/)).
This makes “rubble masonry” a controlled construction family, not permission
for arbitrary Voronoi cells.

A SideFX practitioner seeking a specific irregular wall describes the visible
logic accurately: mostly rectangular/square stones follow horizontal lines,
some courses split, and small stones fill holes beside larger stones. The
artist explicitly rejects a fracture result as too random and erratic
([SideFX forum, *Creating specific stone wall pattern*](https://www.sidefx.com/forum/topic/98082/)).
Another SideFX discussion argues for low-poly stone geometry plus textures for
detail when corners, arches, stone thickness, and non-tiling layout matter
([SideFX forum, *Stone wall generation*](https://www.sidefx.com/forum/topic/53335/)).
These are practitioner observations rather than historical authorities, but
they identify the exact failures of a flat random-cell material.

**Inference for `RubbleMasonry`:** preserve the current row hierarchy as one
“roughly coursed” preset, then add construction rules rather than more edge
noise:

- place a lift on a gravity baseline;
- select stones with a stable lower bearing face;
- reject substantial overlap and unsupported overhang;
- avoid long continuous vertical joints;
- use smaller pinnings/snecks to close residual voids;
- allow occasional course splitting around unusually large units;
- reserve uncoursed/polygonal patterns for lithologies and building traditions
  that support them.

The current width/row tests are a sound start, but should be extended to actual
occupied shapes, contact length, centre-of-mass support, void area, and joint
continuity.

### A thick castle wall is not one textured slab

The medieval Visby city wall study describes three-leaf construction: two
limestone shells in lime mortar surrounding a softer, porous limestone-rubble
and clay-mortar core
([International Masonry Society, *Construction and materials of Visby medieval city wall*](https://www.masonry.org.uk/downloads/id1512-construction-and-materials-of-visby-medieval-city-wall-risk-of-damage/)).
This is a Baltic example, not a universal German castle prescription, but it
demonstrates why a 1.20-metre wall cannot be understood as a single repeated
front-face material. Face stones, through/bond stones, core rubble, and mortar
have different scales and visibility.

Historic England also notes that medieval high-status work developed more
sophisticated dressing, interlocking joints, and shaped arch voussoirs, while
rubble remained common in humbler fabric. Many rubble walls were protected by
lime render. Therefore castles and churches should combine materials by role:

- rubble or roughly coursed field wall;
- larger bond/through stones at structurally meaningful intervals where the
  selected construction uses them;
- dressed quoins, jambs, sills, arches, stringcourses, copings, parapet caps,
  and carved/moulded details;
- a rubble/hearting core visible only in breaks, wall tops under construction,
  or ruins;
- optional render/limewash over field masonry where the building/history calls
  for it.

**Inference:** the texture can represent a sound wall face, but corners,
openings, crowns, damaged sections, and wall cores need geometry-aware material
assignment. Do not project rubble continuously across a dressed quoin or use
the same face pattern on a freshly broken cross-section.

### Stone selection follows local geology

Historic England states that building stones commonly reflect local geology
and that lithology, grain size, sedimentary structures, identification
features, and weathering behavior all matter
([Historic England, *Technical Conservation Guidance and Research*](https://historicengland.org.uk/content/docs/advice/technical-conservation-guidance-and-research-brochure-pdf/)).
Its sourcing guidance requires chemical, physical, and mineralogical
compatibility rather than matching color alone
([Historic England, *Identifying and Sourcing Stone for Repair*](https://historicengland.org.uk/advice/technical-advice/buildings/building-materials-for-historic-buildings/identifying-and-sourcing-stone-for-repair/)).
British Geological Survey work on a castle demonstrates the principle at one
site: its main building stone is a locally sourced sandstone formation, and the
assessment ties source, character, and weathering style together
([BGS, *Source, character and weathering style of building stone in Culzean Castle*](https://nora.nerc.ac.uk/id/eprint/510174/)).

These sources concern Britain, but the physical inference transfers: premodern
transport costs favor local or regionally traded material, and geology is a
better generator input than an unconstrained multicolor palette.

**Inference:** define lithology presets, each with internally coherent shape
and channel behavior:

- bedded sandstone: tabular blocks, aligned bedding, granular faces, iron
  staining where appropriate;
- limestone: warmer/cooler regional palette, fossil/oolitic structure only at
  resolvable scale, solution/weathering behavior;
- granite/gneiss: harder angular or rounded fieldstones, crystalline but
  restrained mineral distribution;
- slatey/metamorphic stone: flatter units and directional cleavage;
- river cobble: rounded units and a different packing/mortar fraction.

A mixed wall can draw from a geologically plausible local deposit or repair
batch, but should not select unrelated warm sandstone, gray granite, and beige
limestone independently for every stone. Use a dominant family, limited
secondary material, and coherent quarry/batch fields.

At 4.688 mm per texel, the recipe can resolve major bedding, centimetre-scale
inclusions, chips, tool contact, granular breakup, and mortar aggregate. Fine
crystals and pores should contribute to bounded albedo/roughness and fade in
mips rather than become high-amplitude height noise.

### Mortar and void filling are part of the construction

Historic pointing guidance says rubble stonework can have larger apparent
joints whose visible width is reduced with small snecked pieces, and stresses
porous mortar that allows the wall to breathe and drain
([Northern Ireland Department for Communities, *Repointing stone and brick*](https://www.communities-ni.gov.uk/articles/technical-note-repointing-stone-and-brick)).
New Forest conservation guidance notes that traditional lime mortar and its
coarser local aggregates were commonly sourced near the work and should remain
weaker and more porous than the masonry
([New Forest National Park Authority, *Pointing*](https://www.newforestnpa.gov.uk/document/pointing/)).

The procedural-art analogue is equally clear. Kyle Horwood's Substance stone-
wall workflow authors stone height, mortar height, roughness, and base color as
distinct stages
([80 Level, *Creating a Procedural Stone Wall in Substance Designer*](https://80.lv/articles/creating-a-procedural-stone-wall-in-substance-designer)).
GDC material-layer practice likewise treats masonry as reusable base materials
plus controlled surface layers rather than one undifferentiated noise stack
([Pettineo, *Crafting a Next-Gen Material Pipeline for The Order: 1886*, GDC 2014](https://media.gdcvault.com/GDC2014/Presentations/Pettineo_Matt_Crafting_A_Next-Gen.pdf)).

**Inference:** mortar should be generated from the complement of actual packed
stone shapes, then given its own aggregate, application, pointing, shrinkage,
and weathering. Small pinnings should be explicit stone identities embedded in
wide joints, not mortar-colored noise. Avoid deep empty black gaps in a sound,
mortared wall: broad voids should be filled or describe a damaged/ruined preset.

### Weathering follows lithology and exposure

Stone deterioration differs by mineralogy, porosity, bedding, and exposure.
Historic England's sourcing process explicitly includes diagnosing why the
original stone is deteriorating. Research on Caen limestone shows that
petrographically different replacement stones develop different long-term
appearance even in similar environments
([Rozenbaum et al., *Preliminary investigations into Caen Stone in the UK*](https://doi.org/10.1016/S0360-1323(03)00075-1)).
A broader building-stone review lists blistering, crumbling, flaking, granular
disintegration, color deepening, iron staining, copper runoff, and biological
green staining as distinct observed mechanisms
([Gomez-Heras et al., *A Geological Perspective on Building Stone Deterioration*](https://www.mdpi.com/2073-4433/11/8/788)).

**Inference:** a clean tile should contain only intrinsic stone and restrained
micro-weathering. Building/world masks should place:

- rising damp, salts, splash erosion, and biological growth near persistent
  moisture;
- rain streaks beneath copings, gutters, openings, and projections;
- leached mortar and calcite runs below joints;
- soot near combustion sources and sheltered urban recesses;
- frost/spall damage according to exposed orientation, water paths, and
  lithology;
- vegetation primarily in genuinely open, damp joints of ruins, not every
  sound castle wall.

Maintained inhabited walls and ruined walls need separate presets. Age alone
does not justify universal missing mortar, moss, and black cavities.

## Channel relationships

Stone and lime/earth mortar are dielectric, so metalness remains zero. Channels
should derive from shared causes without becoming identical.

- Packed unit silhouette and depth determine the main height/normal/AO break.
- Lithology controls plausible albedo family, grain, bedding, intrinsic
  roughness, fracture style, and weathering response together.
- A tilted but intact face changes height/normals, not necessarily albedo.
- Fresh chips expose a lithologically related interior, change height, create
  local AO at the recess, and may be rougher than a weathered outer face.
- Mortar color comes from binder and aggregate; its height comes from bedding
  and pointing; its roughness/porosity should not be copied from adjacent stone.
- Damp darkens albedo and usually lowers apparent roughness; efflorescence or
  lime leaching lightens and roughens; biological films can darken/green and
  smooth or roughen depending on thickness.

The 32 mm declared relief should be reserved for meaningful face projection,
recessed joints, and chips. Broad stone faces should not inflate into uniformly
rounded pillows. The current near-planarity test is valuable and should remain.
AO must weaken with distance and should not be baked into albedo as permanent
black joint lines.

## UVs, walls, and castle integration

On a flat wall, U should follow accumulated façade-run distance and V physical
height. The material binding must account for the 4.8 metre tile, not inherit
the generator's generic 2 metre repeat. Continuous wall sections should share
coordinates so courses do not restart at every triangle or segment.

For round towers and curved curtains, use arc length for U so units do not
stretch around curvature. At corners, openings, wall tops, battered bases, and
broken sections, a flat repeating projection is insufficient. Use explicit
modules or generated geometry for larger face stones and dressed boundaries,
with the surface recipe supplying intra-stone detail.

Castle material assignment should distinguish:

- rubble/roughly coursed face masonry;
- dressed stone at structural/architectural edges;
- core/hearting material;
- mortar/render/limewash finish;
- damage or ruin exposure.

This should be represented directly in the clean building schema rather than
inferred from generic `FortifiedMasonry` or preserved through compatibility
fallbacks.

## Tiling, mips, distance, and LODs

A 4.8 metre repeat is generous, but 22 horizontal bands can reveal a square
period through recurring large stones, color sequence, and course boundaries.
Use deterministic façade seeds, compatible edge states, or a small Wang-like
variant set to extend the wall without breaking gravity or joint continuity. A
procedural Wang-tile wall algorithm is relevant precedent for bounded,
non-repeating wall patterns
([Kopf et al., *Procedural Wang Tile Algorithm for Stochastic Wall Patterns*](https://arxiv.org/abs/1706.03950)).

Semantic mips should:

- downsample albedo in linear light and re-encode to sRGB;
- decode, filter, and renormalize normal vectors;
- transfer unresolved normal variance into roughness;
- preserve average stone/mortar/pinning coverage;
- prevent thin joints and tiny pinnings from flickering or collapsing into a
  dark grid;
- filter height without allowing isolated deep gaps to depress a whole coarse
  texel;
- reduce AO as joints become unresolved.

LOD0 may retain explicit corner/opening stones, large face relief, pinnings,
and major damage. LOD1 should preserve course/packing rhythm, major unit
colors, dressed boundaries, and broad joint depth with reduced microdetail.
LOD2 should retain stable low-frequency masonry structure and lithology tone;
fine joints, grains, and chips should merge. Silhouette geometry still owns
crenellations, broken crowns, wall batter, and large displaced stones. All LODs
must preserve average color and stable unit placement through transitions.

## Recommended implementation sequence

### 1. Define geology and construction presets

- Select world-region lithology families and source their palettes, bedding,
  fracture, and weathering behavior.
- Retain the current algorithm as a roughly coursed preset, not the universal
  rubble pattern.
- Separate wall face, thick core, dressed boundary, render, and ruin roles.
- Reconcile 4.8 metre material scale with 2 metre mesh UVs.

### 2. Upgrade from row partition to constrained packing

- Generate candidate stones with lithology-appropriate aspect ratios.
- Place them on gravity lifts with collision/non-overlap checks.
- Enforce bearing/contact and centre-of-mass support, limit continuous vertical
  joints, and insert pinnings into bounded residual voids.
- Provide controlled rough-coursed, polygonal/rag, tabular, and cobble variants
  only where appropriate.

### 3. Build stone and mortar as causal materials

- Give each stone a coherent lithology-derived face, interior, bedding,
  fracture, tilt, and roughness response.
- Derive mortar geometry from packed gaps and add independent binder,
  aggregate, pointing, shrinkage, and repair parameters.
- Keep pinnings as explicit stone units.

### 4. Integrate castle-specific geometry and finishes

- Assign `RubbleMasonry` and `DressedStone` by role at quoins, openings,
  crowns, copings, and details.
- Add core/hearting and render/limewash materials for cutaways, ruins, and
  historically selected intact surfaces.
- Use arc-length coordinates on towers and continuous coordinates on curtains.

### 5. Add building-space weathering and semantic mips

- Drive damp, runoff, salts, soot, biological growth, and ruin damage from
  exposure and lithology.
- Correct albedo/normal filtering, compensate roughness, and preserve packing
  statistics through the LOD chain.

## Acceptance and regression tests

### Deterministic numeric tests

- Assert tile scale, texel scale, unit-size/aspect distributions, joint widths,
  projection depth, and pinnings in metres.
- Rasterize occupied shapes and prove no overlaps above tolerance, bounded
  residual void area, and no mortarless holes in sound presets.
- Measure bearing contact length and prove each non-foundation stone's centre
  of mass lies within supported bounds.
- Bound vertical-joint continuity and test that broad stones bridge multiple
  lower units without unsupported ends.
- Prove pinnings occupy qualifying wide joints and do not overlap primary
  stones.
- Prove each lithology preset stays within its reviewed palette/mineral family
  and changes shape/weathering rules coherently, not albedo alone.
- Prove mortar binder/aggregate fields are independent of neighbouring stone
  IDs.
- Verify continuous U/V scale across façade meshes and arc-length scale on
  towers.
- Verify zero metalness, causal channel bounds, linear-light albedo mips,
  decoded/renormalized normal mips, coverage stability, and deterministic LOD
  selection.

### Visual fixtures

Review under neutral, overcast, wet, and grazing light:

- a measured single lift annotated in centimetres, showing bearing and
  pinnings;
- rough-coursed, polygonal/rag, tabular, and cobble presets side by side, each
  labeled with lithology and intended region/use;
- a long flat wall, right-angle corner, round tower, battered curtain, jamb,
  arch, coping, crenellated crown, and broken cross-section;
- rubble field beside dressed quoins/opening stones and an optional rendered
  field;
- intact maintained, weathered, repaired, and ruined variants;
- isolated damp, runoff, lime leaching, frost/spall, soot, and biological masks;
- a 2 by 2 tile/repeated-wall view plus matching LOD0/LOD1/LOD2 transitions in
  stationary and slow camera motion.

An independent visual reviewer should reject floating or overlapping stones,
long stacked vertical joints, implausible unsupported overhangs, arbitrary
Voronoi cells, gaps without pinnings/mortar, inflated pillows, equal unrelated
rock colors, mortar derived from stone color noise, rubble projected through
dressed quoins, square repeats, stretched tower stones, universally ruined
castles, black AO-painted joints, or LOD changes that alter course scale and
average wall color.

## Evidence, inference, and project decisions

- **Evidence:** historic rubble ranges from closely jointed random/rag work to
  roughly squared courses and often used protective lime finishes; construction
  logic includes bearing, bounded joints, small infill stones, mortar, and in
  some thick walls distinct face leaves and rubble cores; local geology strongly
  determines stone character and weathering; practitioners separate stone
  layout, stone material, and mortar rather than relying on noise alone.
- **Inference:** the affordable robust solution is lithology-aware constrained
  packing plus explicit mortar/pinnings, façade-aware dressed boundaries, and
  building-space weathering. The present variable-row algorithm is a useful
  roughly coursed baseline but is not a physical packing or a complete castle
  wall system.
- **Repository decisions still required:** German world-geology inputs; which
  construction presets apply to houses, foundations, churches, curtains, and
  towers; face/core/dressed/render schema; whether close LOD uses explicit
  stones at corners/openings; palette mapping; runtime material binding; and
  how LOD2 encodes the same packing statistics.

The minimum credible milestone is a real-scale castle-wall fixture where
local-lithology stones demonstrably bear without overlap, wide gaps contain
plausible pinnings and independent lime mortar, dressed quoins and openings
interrupt the rubble correctly, a broken section reveals distinct face and
core construction, and the wall converges stably through all LODs.
