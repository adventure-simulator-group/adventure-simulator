# Clay roof tile procedural texture prior art

## Scope

This report concerns exactly the `ClayRoofTile` procedural surface: a fired-clay
roof covering for late-medieval and early-modern German buildings, currently
described by the implementation as overlapping plain `Biberschwanz` tiles. It
addresses historically plausible tile forms and layouts, overlap construction,
handmade and fired variation, damage and biological deposits, the division
between geometry and texture, causal PBR channels, physical scale, roof UVs,
tiling, mipmaps, and building LODs.

It does **not** establish one red beaver-tail material as the roof of every 1544
German building. Roofing was regional, economic, and building-specific. The
project already has separate slate and timber-shingle recipes; clay itself also
needs at least regional/profile presets rather than one universal pattern.

## Repository facts and constraints

The following are facts observed in this worktree, not claims from external
sources.

- `ClayRoofTile` generates 512 x 512 albedo, OpenGL tangent-space normal,
  normalized height, and ARM textures. It declares a 2.4 m square repeat and an
  18 mm represented height range.
- The analytic pattern contains 15 tiles across and 16 visible courses down the
  repeat. Its tests therefore describe a 160 mm visible tile width and 150 mm
  visible course exposure. Alternate courses are shifted half a tile.
- The tail is a rounded analytic cut beginning about two-thirds of the way down
  each visible tile. A recessed lower course is exposed between tails. Per-tile
  yaw, vertical offset, asymmetric tail width, cup, twist, thickness, and two
  sinusoidal face frequencies introduce handmade variation.
- The implementation uses a largely uniform dark-red clay base. A broad
  texture-space firing field and per-tile hash shift albedo and roughness. Edge
  wear is sparse and brightens the albedo. Contact recesses reduce AO.
- No cracks, missing or slipped tiles, peg/nib evidence, sanded or hand-struck
  surface texture, lichen, moss, soot, mineral bloom, glaze, ridge treatment,
  verge/eave treatment, or roof-space exposure mask is represented.
- Tests cover deterministic periodicity, plausible visible width/exposure and
  height range, recessed overlap coverage, nonmetallic ARM data, mip count, and
  deterministic review exports. The exporter creates base, 2 x 2, 128 px, and
  64 px views, but it does not exercise an oblique roof in the tactical renderer
  or compare LOD transitions.
- The shared `image_rgba_mipped` helper averages all four byte channels. It
  therefore averages sRGB albedo in encoded space, averages tangent normals as
  colors without semantic reconstruction, and averages perceptual roughness
  without accounting for unresolved normal variance.
- Building detail and every building LOD currently assign roof UVs from world
  `X,Z / 2.0 m`. The texture's own declared repeat is 2.4 m, so binding it to
  that mesh contract would shrink the intended 160 x 150 mm visible rhythm to
  about 133 x 125 mm.
- More importantly, world-planar `X,Z` mapping does not establish one axis
  across the roof and one axis down its slope. Roof orientation and pitch can
  rotate, shear, or compress the apparent courses. The recipe's promise that V
  increases down-slope is not upheld by the current mesh UV contract.
- The tactical building material setup does not yet consume
  `procedural_textures.clay_roof_tile`. Clay roofs still receive a per-building
  two-color checker through a generic opaque material, with no generated normal
  or ARM map.
- Roof faces are closed, thickened planar prisms. The generator has semantic
  roof faces, cutouts, dormers, enclosure faces, and multiple materials, but the
  field covering itself has no tile geometry at eaves, verges, ridges, hips,
  valleys, or openings.

These constraints mean that improving only the pixels would not yet improve the
city. Physical scale, slope-aligned UVs, material binding, edge geometry, and
semantic mip generation are part of the same acceptance contract.

## Historical evidence

### A beaver-tail preset is plausible, but it is regional rather than universal

The University of Bamberg's archaeological building-material typology states
that hollow monk-and-nun tiles predominated until about 1300, flat tiles became
more common afterward, and pointed “Gothic” beaver-tail forms occur more often
from the fourteenth century. It also treats intentional corner pieces as
archaeologically meaningful special forms
([Bamberg archaeological typology, *Dachziegel*](https://amanz-balismink.rproxy.rz.uni-bamberg.de/balismink/index.php/Dachziegel)).
A German roof-tradition survey places the beaver-tail's emergence around
fourteenth-century Nuremberg and characterizes it as particularly southern and
eastern German, while hollow tiles remained important elsewhere
([Netzwerk Steine in der Stadt, *Traditionelle Ziegel- und Metalldachdeckungen*](https://www.steine-in-der-stadt.de/downloads/Publikationen-SidS/SidS_1%282018%29.pdf)).

The transferable conclusion is not “replace the current beaver tail.” It is:

- retain it as a credible southern/eastern or Nuremberg-influenced clay preset;
- add pointed, straight, or other documented foot cuts as controlled regional
  variants rather than random per-tile silhouettes;
- reserve monk-and-nun, hollow-pan, and other profile families for distinct
  recipes or geometry because their cross-sections and water-shedding logic are
  materially different;
- select roof family from settlement region, date, building status, pitch, and
  supply rather than facade color.

The historic-roof archive reports that round-cut beaver tails were preferred
from the sixteenth century, while the archaeological source places pointed
forms earlier and from the fourteenth century onward
([Dachziegelarchiv, historic flat-tile forms](https://www.dachziegelarchiv.de/mod_detail.php?mod_id=53&sei_order=suchname_asc&sei_page=83&show_sei_id=15271)).
For a game fixed in 1544, round and pointed presets can therefore coexist, but
their distribution should be authored at roof or district scale, not sprinkled
within one slope without evidence.

### Plain tiles are overlapping systems, not a printed grid

SPAB describes plain clay tiles as gently cambered and double-lapped: each tail
overlaps two courses below. Earlier handmade tiles were hung with wooden pegs,
varied locally, and had substantially more individuality than later
machine-pressed products
([SPAB, *Clay plain-tiled roofs*](https://www.spab.org.uk/advice/clay-plain-tiled-roofs)).
Historic England's craft sequence explains why: wet clay was sanded to prevent
sticking, peg holes were made by hand, drying shrinkage could reach roughly ten
to twelve percent depending on clay, and tiles were shaped on curved boards or
racks before firing. The resulting camber helps adjacent tiles fit and limits
capillary draw
([Historic England, *Practical Building Conservation: Roofing*](https://historicengland.org.uk/images-books/publications/roofing-conservation/roofing-marketing-spread/)).

This supports the implementation's overlap, camber/cup, and restrained unit
variation. It also exposes missing structure:

- course exposure is only the visible part of a much longer tile; height masks
  must preserve which course is on top rather than blend equal-height “cells”;
- head lap and side joints are distinct. A tail opening must reveal a valid
  under-course, never background or a dark mortar-like void;
- the half-course shift is plausible for a double-lapped field, but edge pieces
  and course termination must be solved at verges and openings;
- handmade variation should include low-frequency camber, slight thickness and
  outline variation, peg/hand-process evidence where visible, and restrained
  bow/twist—not independent random rotation large enough to break drainage;
- a roof field needs dedicated ridge, hip, valley, eave, verge, abutment, and
  dormer transitions. Repeating the field texture through them is not a valid
  construction.

SPAB gives 229 x 152 mm to 305 x 203 mm as the range of surviving English plain
peg tiles, which is useful evidence for handmade variability but not a German
1544 dimension standard. Contemporary German conservation products commonly
sit near 180 x 380 x 18 mm with about 145–165 mm cover length
([Creaton heritage beaver-tail specification](https://www.dachdecker-pahl.de/produkte/creaton/biberschwanz/antik/)).
The current 160 mm visible width, 150 mm exposure, and 18 mm relief are therefore
a reasonable **working game scale**, but should remain a tunable preset rather
than be labeled an exact archaeological reconstruction.

### Fired variation has causes and scale

The handmade-tile craft sequence produces variation from raw clay, sand,
forming, drying, kiln position, firing atmosphere, and reuse. SPAB's medieval
roof repair project describes handmade tiles as slightly irregular with a
gentle mixture of colors and textures; its replacement firing trials produced a
palette from light orange to deep red rather than arbitrary hues
([SPAB Old House Project, *Roof tiles for our Old House Project*](https://www.spab.org.uk/news/roof-tiles-our-old-house-project)).

For `ClayRoofTile`, use correlated variation at three scales:

1. **Clay body / batch:** one restrained mineral and base-hue family across a
   roof or repaired patch.
2. **Kiln batch / firing zone:** broad clusters of warmer orange, red, deep red,
   occasional darker reduction or overfire—not a smooth world-space color wave
   that visibly repeats every 2.4 m.
3. **Individual tile:** small value, saturation, surface-pore, sand-drag, and
   edge differences, correlated across albedo and roughness.

Glazed decorative tiles existed, but should be a distinct high-status preset.
The RDK roof survey records green, yellow, brown, white, and black glazed beaver
tails arranged in patterns in southwestern German and Alpine regions from at
least the fourteenth century
([RDK Labor, *Dach*](https://zikg000-rdklabor.srv.mwn.de/wiki/Dach)).
That is evidence for deliberate patterned roofs, not justification for random
multicolored ordinary houses.

## Practitioner workflows

### Build the physical overlap in height first

Procedural material artists consistently establish tile profile and layout in
height before deriving secondary maps. Daniel Thiger's roof-tile workflow begins
with reference breakdown, tile profile, and roof construction, then adds cracks,
surface damage, color blocking, broken tiles, and roughness
([Thiger, *Creating Roof Tiles in Substance Designer*](https://levelup.digital/l/levelup_rooftiles)).
Federico Guerra similarly separates height and color workflows and exposes
dedicated subgraphs for profile selection, broken tiles, and moss; normal, AO,
curvature, and roughness follow from those structures
([Guerra, *Procedural Roof Tiles Material*](https://federicoguerra.artstation.com/projects/qAz2q2)).

That validates a causal graph for this Rust recipe:

`layout -> top-course ownership -> profile/camber -> chip/crack masks -> height`

then

`height + material/process masks -> normal, AO, roughness, albedo`.

The current analytic ownership/recess model is a strong foundation. It should
be made explicit enough that every texel carries a stable tile ID, course ID,
top/under ownership, side-joint distance, tail distance, contact depth, and
damage mask. These masks should be shared across channels rather than inferred
independently from noise.

### Geometry placement solves boundaries that a tileable material cannot

SideFX practitioners building roofs with copied tile geometry construct a grid
from real tile size, orient it to each roof primitive, stagger rows, and identify
line endpoints for special edge handling
([SideFX, *Tile/roof builder*](https://www.sidefx.com/forum/post/208070/);
[SideFX, *How to project rooftiles on a primitive?*](https://www.sidefx.com/forum/topic/50916/?page=1)).
The forum examples are not historical authorities, but they expose the same
production constraints as physical roofing: slope basis, exact spacing,
alternate-course offset, half/special edge pieces, and non-generic boundaries.

A practitioner hybrid workflow models several tile variants, sculpts corner
damage, lays them into a seamless patch, bakes height/ID/AO/curvature/normal to
a plane, and then authors material channels from those bakes
([Baer, *The advantages of Substance Designer as PBR texturing tool*](https://www.artstation.com/sebastianbaer/blog/pn9e/the-advantages-of-substance-designer-as-pbr-texturing-tool)).
The transferable idea is not that Adventure Simulator needs offline bakes. It
is that the field pattern may be texture-level while silhouette-critical and
construction-critical parts remain geometry.

## Geometry versus texture division

### Texture responsibilities

The tileable material should own:

- ordinary interior field courses on broad roof planes;
- top/under-course overlap relief and narrow inter-tile contacts;
- gentle camber, bow, hand-struck/sanded microtexture, pores, and firing marks;
- sparse small chips and hairline cracks that do not change the roof silhouette;
- causal AO at overlaps, nonmetallic roughness, and restrained batch variation;
- low-frequency exposure masks supplied in roof-local or building-local space.

### Geometry responsibilities

LOD0 geometry or dedicated roof trim should own:

- the stepped tail silhouette at eaves and exposed rakes/verges;
- ridge and hip caps, valleys, flashing/abutment transitions, gutters, and roof
  penetrations;
- dormer and chimney intersections;
- conspicuously slipped, missing, lifted, or broken tiles when gameplay-camera
  distance can resolve the opening;
- profile families whose cross-section changes the silhouette, especially
  monk-and-nun or deeply curved hollow tiles.

A cheap implementation does **not** require one mesh per tile across every
roof. Use a flat/prismatic roof field plus small repeated or generated edge
strips at eaves and verges, distinct ridge/hip pieces, and perhaps a few sparse
damage decals or replacement units. LOD1 can collapse the edge strip into a
coarser sawtooth or baked silhouette. LOD2 should retain only roof outline,
course-direction color/normal energy, and broad firing/weathering masses.

This division avoids spending triangles where parallax and self-occlusion are
already represented by the normal/height field, while preventing the visibly
flat eave and texture-through-ridge failures that no PBR map can hide.

## Damage, moss, soot, and exposure

SPAB identifies frost action, worsened where moss retains moisture, wind-lift,
slipped or broken tiles, and failed fixings or underlying supports as distinct
deterioration paths. Historic England notes that ridges, gables, and chimney
abutments are commonly among the first roof areas to require attention
([Historic England, *Repair or renew the roof in an older home*](https://historicengland.org.uk/advice/your-home/maintain-repair/roofs/)).

Therefore weathering must not be a uniform texture-space noise layer:

- **Moss/algae/lichen:** favor shaded, persistently damp, less freely drained
  roof regions and overlap/contact recesses. Moss has slight height, darker or
  greener albedo, and high diffuse roughness. Keep it patchy and building-space
  stable. Mark Foreman's Substance technique starts from tile-edge/contact masks
  and grows moss outward, a useful controllable construction
  ([Adobe Substance, *Moss between blocks and roof tiles*](https://www.adobe.com/learn/substance-3d-designer/web/mark-foreman-s-substance-3d-designer-tips-and-tricks)).
- **Soot:** place from chimney position, prevailing/down-roof flow, and roof
  geometry. It should darken albedo and may change roughness, but must not be a
  periodic spot generated inside every material tile.
- **Frost/spall/chips:** correlate with moisture-retaining edges, exposed tails,
  and flaws. Keep most damage shallow. A bright rim should represent exposed
  clay body only where fracture geometry exists, not generic edge wear.
- **Slips and missing units:** geometry or instance state, not height-map holes.
- **Repairs:** roof-scale clusters with compatible but discernibly different
  batches. Reused tiles make mild mixed-age fields plausible; salt-and-pepper
  random color does not.

At 1544, avoid making every roof a picturesque centuries-old ruin. Age and
maintenance should follow building status and roof age; soot should require a
source; heavy moss should require exposure conditions.

## Causal channel construction

### Height and normal

- Compose height in metric layers: course overlap/profile first; unit camber
  and twist second; forming/sand/tool surface third; sparse cracks/chips/moss
  last.
- Keep the 18 mm represented range tied to plausible tile thickness and lap
  relief. Do not spend the entire range on noise.
- Generate tangent normals from the final physical-height field with the actual
  metres-per-texel. At roof boundaries, geometry normals remain authoritative.
- Preserve low-frequency camber and overlap at distance; discard pore and sand
  microstructure before it aliases.

### Albedo

- Use clay batch, firing, inclusions, efflorescence, soot, growth, and fresh
  fracture masks. Do not multiply baked directional light or AO into base color.
- Keep ordinary fired clay within one local orange/red/brown family. Roof-wide
  batch patches should be broader than the 2.4 m tile repeat.
- A fresh chip may expose a slightly lighter, more saturated porous body, but
  this follows the chip mask and should be sparse.

### Roughness, AO, and metallic

- Fired clay remains dielectric (`metallic = 0`).
- Roughness varies with clay body, firing/vitrification, surface sand and pores,
  moisture, soot, lichen, glaze, and fresh fracture. It should not simply copy
  albedo luminance.
- AO belongs primarily in tight top/under contacts and deep damage. Do not bake
  broad roof-facing shadows into AO or albedo.
- Curvature may help localize edge wear and moss seeds, but it is an authoring
  mask, not a universal whitening filter.

Guerra's explicit split between height and color while deriving secondary maps
supports this shared-mask architecture. Ready at Dawn's GDC material pipeline
also treats unresolved normal variation as a roughness/filtering problem rather
than independently averaging each channel
([Pettineo, *Crafting a Next-Gen Material Pipeline for The Order: 1886*, GDC 2014](https://media.gdcvault.com/GDC2014/Presentations/Pettineo_Matt_Crafting_A_Next-Gen.pdf)).

## UV and physical-scale contract

Every roof face needs a local orthonormal basis:

- `V` points down the steepest descent direction in the roof plane;
- `U` is perpendicular to V within the plane, following the course;
- UV distance is measured on the sloped surface in metres, not projected plan
  distance;
- adjoining fragments of the same semantic slope share an origin so
  tessellation and dormer cuts do not restart the pattern;
- opposing slopes may share course scale but have separate origins and drainage
  direction;
- curved/conical roofs require arc-length U and slope-length V, with a seam
  placed deliberately at a low-visibility radial boundary.

The recipe should expose physical visible width and course exposure directly,
or at least bind mesh UVs using `CLAY_ROOF_TILE_TILE_METRES`. The current 2.0 m
generic repeat silently violates the recipe's 2.4 m scale. A test should fail if
recipe scale and mesh scale diverge.

Roof-space weathering requires a second coordinate domain or explicit per-roof
parameters. Do not distort the tile field to place moss and soot. The field UV
can repeat; the exposure masks should be continuous across the roof assembly.

## Tiling, mipmaps, distance, and LOD

### Tiling avoidance

The repeated physical rhythm is real, but repeated **variation** is not. Keep
course and tile spacing periodic while decorrelating:

- roof-level clay batch and repair-patch masks;
- kiln clusters larger than or incommensurate with the base repeat;
- sparse damage chosen by stable tile identity in roof coordinates;
- moss, soot, and wetness from building-space exposure;
- a small deterministic set of compatible field variants, without rotating the
  gravity-dependent overlap pattern.

Do not rotate or mirror the entire texture as a cheap anti-tiling measure: that
would turn courses uphill or sideways. Long-roof review must expose the full
repeat under neutral light.

### Semantic mip generation

The existing byte average is insufficient. Generate each mip according to its
meaning:

- filter albedo in linear light, then encode sRGB;
- average/reconstruct normals semantically, or derive every mip's normal from a
  filtered metric height field;
- preserve mean height/overlap without inventing intermediate ownership at
  course discontinuities;
- filter AO as visibility, not gamma color;
- raise effective roughness when subpixel normal variance disappears. The GDC
  Ready at Dawn talk explicitly modifies roughness to reduce specular aliasing;
  NVIDIA's texture tools likewise expose slope-space normal mipmapping
  ([NVIDIA Texture Tools Exporter](https://developer.nvidia.com/texture-tools-exporter?mobile-app=true&theme=dark)).

The target failure mode is not only static blur. Oblique roofs cover many
pixels in one axis and few in the other, so course lines and tail arcs readily
produce crawling moire under motion. Anisotropy 8 is a useful runtime default,
but cannot repair semantically incorrect source mips.

### LOD behavior

- **LOD0 / near:** slope-aligned full material; eave/verge/ridge/hip geometry;
  normal retains overlap, camber, and selected damage; building-space deposits.
- **LOD1 / middle:** coarser edge treatment; normal retains course relief but
  drops pores and hairline cracks; albedo keeps roof batch and broad repair or
  weathering masses.
- **LOD2 / far:** stable roof silhouette and low-frequency clay color; perhaps
  one subdued course-direction normal/roughness signal. Individual tail arcs
  must not shimmer or become a checker.
- LOD transitions should preserve average albedo, average roughness, roof
  outline, and the direction of the course rhythm. Geometry and texture should
  cross-fade or switch where their apparent frequencies match.

## Recommended implementation sequence

### 1. Define historically bounded presets

Keep `BiberschwanzRound` as a southern/eastern sixteenth-century-capable preset;
add at least `BiberschwanzPointed` and reserve separate future families for
monk-and-nun/hollow profiles. Store visible width, exposure, nominal full
length, thickness, tail form, lap mode, clay batch, and glaze status explicitly.

### 2. Repair the roof coordinate contract

Generate slope-local metric UVs in both high detail and all LOD meshes. Bind the
actual procedural texture maps in tactical materials. Add a test that V points
down-slope and that one metre of roof surface produces one metre of texture
distance regardless of roof yaw or pitch.

### 3. Make the analytic graph causal

Retain stable tile/course ownership. Add hand-process microstructure, controlled
profile variants, sparse damage masks, and causal albedo/roughness/AO. Replace
the periodic firing wave with roof/batch parameters plus smaller per-unit
variation.

### 4. Add semantic roof boundaries

Generate cheap eave/verge strips and ridge/hip/valley/abutment treatments from
the existing roof assembly. Keep the broad field texture-based. Do not model
every tile unless profiling shows that a near-view instance approach is cheap
enough.

### 5. Add exposure and semantic mips

Supply roof-space masks for shade/damp, chimney soot, runoff, repair age, and
growth. Generate linear-color, height-derived normal, AO, and variance-aware
roughness mips, then validate LOD transitions under motion.

## Acceptance and regression tests

### Deterministic numeric tests

- The pattern is bitwise deterministic and periodic only in its field channels.
- Preset visible width, exposure, thickness, full tile length, and lap mode stay
  in declared ranges. The current 160 mm x 150 mm x 18 mm working contract is
  not silently rescaled by a generic mesh repeat.
- Every exposed tail sample resolves to a valid under-course; no background
  holes or equal-height overlaps occur.
- Course ownership is monotonic down-slope, alternate offsets are exact modulo
  bounded handmade deviation, and drainage-breaking rotations are rejected.
- Metric roof UV tests cover north/south/east/west slopes, multiple pitches,
  clipped/dormer faces, and conical arc-length mapping.
- Metallic is zero. AO minima coincide with actual contacts/deep damage.
  Albedo, roughness, and height changes share the appropriate causal masks.
- Weathering masks are stable in roof/building coordinates and do not repeat at
  the 2.4 m field period.
- Albedo mips are filtered in linear light; normal mips are unit-valid after
  reconstruction; roughness does not decrease when unresolved normal variance
  increases.
- Every mip and LOD remains within bounded mean albedo, roughness, and normal
  energy so transitions do not flash.

### Visual fixtures

Capture, under fixed neutral and grazing light:

- flat round-tail and pointed-tail reference slabs at true scale with a metre
  ruler and side profile;
- a 2 x 2 repeat and a 20 m long slope, looking for firing blobs, identical
  damage, seams, and course drift;
- the same gable roof at several yaw angles and pitches, proving constant tile
  scale and down-slope V;
- eave, verge, ridge, hip, valley, chimney, dormer, and roof-abutment closeups;
- clean/new, maintained/old, repaired, damp-shaded, and chimney-sooted variants
  whose causes remain legible but restrained;
- LOD0/LOD1/LOD2 at matched screen sizes and a slow camera approach/retreat,
  including oblique anisotropic views;
- a city overview with multiple regional/material presets, verifying that clay
  roofs read as coherent masses rather than one repeated red checker.

An independent visual reviewer should reject sideways/uphill courses, pitch-
dependent scale, uniform machine-perfect tiles, random rotation that breaks
lap, black mortar-like gaps between tails, every edge highlighted, color noise
uncorrelated with clay/firing, periodic moss or soot, flat eaves with printed
tails, field texture crossing ridges and dormer intersections, over-deep pillow
relief, sRGB-darkened mips, sparkling normals, moire, or LOD color flashes.

## Evidence, inference, and project decisions

- **Evidence:** flat and beaver-tail tiles were present in German regions by the
  late medieval period; pointed forms occur from the fourteenth century and
  round cuts are associated with the sixteenth century; beaver tail was
  particularly characteristic in southern/eastern Germany rather than being
  universal. Plain tiles use overlapping courses, handmade units show camber
  and local variability, and moisture, moss, frost, wind, fixings, ridges,
  gables, and chimney abutments produce distinct deterioration patterns.
- **Practitioner evidence:** production workflows establish profile and overlap
  in height, derive correlated maps, parameterize profile/breakage/moss, orient
  repeated geometry to roof primitives at real spacing, and treat edge pieces
  separately. GDC rendering practice accounts for normal variance in roughness
  filtering to suppress specular aliasing.
- **Inference:** the cheapest robust system is a slope-aligned texture field
  plus sparse boundary geometry and building-space exposure masks. Stable
  handmade variation should be small enough to preserve drainage, while roof-
  scale batches and repairs break repetition. Semantic mips and LOD-specific
  frequency budgets are necessary for a dense city.
- **Repository decisions still required:** the exact 1544 region-to-profile
  distribution; social-status and building-type weights; whether the current
  160 x 150 mm visible scale is retained per preset; the schema for roof-local
  exposure inputs; the edge-strip triangle budget; conical-roof seam rules; and
  how much roof damage is appropriate for normal maintained buildings.

The minimum credible milestone is not a prettier standalone square. It is a
true-scale round- or pointed-tail field bound to an actual pitched roof with V
down-slope, correct double-lap reading, a non-flat eave and valid ridge/dormer
transitions, restrained causal firing and weathering, and stable appearance
through oblique motion and the complete LOD chain.
