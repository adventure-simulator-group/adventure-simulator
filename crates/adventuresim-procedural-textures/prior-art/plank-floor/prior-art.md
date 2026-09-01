# PlankFloor prior art

## Scope and evidentiary limits

This internal report concerns the `PlankFloor` procedural texture for a German
setting around 1544. It focuses on a plain structural board floor, not later
parquet, engineered flooring, or a modern decorative plank product.

Direct, well-published measurements from surviving sixteenth-century German
domestic floors are difficult to find in freely accessible sources. The
strongest historical construction evidence found here comes from official Dutch
heritage guidance, English medieval-floor conservation guidance, German
conservation references cited by those sources, and surviving-building practice.
The Netherlands and England are useful northwestern-European comparisons, not
proof that every German town used identical dimensions or joints.
Recommendations below separate source evidence, inference, and repository
decisions.

## Repository baseline

The existing recipe is a deterministic 1024 px, 7.2 m square field with a
declared 10 mm height range. It models twenty-two longitudinal boards, giving
nominal widths around 0.33 m, and twelve 0.6 m joist stations. It already
contains several valuable construction constraints:

- Board widths vary while remaining broad.
- Board boundaries share ownership and have restrained longitudinal warp.
- Each board has one butt-joint station, and adjacent boards are prevented from
  clustering at the same station.
- Butt joints and most nails are constrained to the implicit joist grid.
- Grain offers rift-like, flat-sawn cathedral, and quiet-face modes.
- Checks, pores, hand marks, cup, height, contact AO, wear, and roughness are
  deterministic and separately represented.
- Tests cover repeatability, periodicity, broad widths, joist-aligned joints,
  sparse gaps/fasteners, channel completeness, mip presence, and zero metallic.

The recipe is not yet the tactical floor material. The tactical building
material setup still creates a simple checker texture for
`BuildingLodMaterial::Floor`. Generated building mesh UVs use world XZ divided
by a generic 2.0 m repeat, whereas the recipe declares a 7.2 m repeat. Sampling
it through the current generic contract would compress its nominal 0.33 m boards
to roughly 92 mm and its 0.6 m joist interval to roughly 167 mm. World-aligned
UVs also let one room inherit the same board field and traffic band as every
other room.

The shared mip helper averages encoded RGBA bytes. Consequently sRGB albedo is
not filtered in linear light, normal-map vectors are averaged as encoded colors
rather than decoded and renormalized, and roughness does not incorporate
unresolved normal variance. These are repository facts, not historical claims.

## Historical construction evidence

### Broad boards, not modern strips

The Dutch national heritage service describes the earliest and most common
historic wooden floor as boards laid directly on beams and left visible. It
states that medieval boards were usually oak, with pine examples appearing from
the late fifteenth century and pine becoming dominant only in the early
seventeenth century. It also records unfinished floors and finishes including
wax, soap, resin, and linseed oil.
[[Rijksdienst voor het Cultureel Erfgoed, *Houten vloeren*](https://kennis.cultureelerfgoed.nl/index.php/Houten_vloeren)]

The Society for the Protection of Ancient Buildings reports that medieval
floorboards were predominantly oak, riven, axed, or pit-sawn, commonly varying
within one floor and reaching 450 mm or more in width. It says boards were often
laid parallel to and rebated into the upper edges of heavy, flat-laid joists. It
dates widespread softwood later and tongue-and-groove boards to the nineteenth
century in its British context.
[[SPAB, *Caring for old floors*, pp. 7–9](https://www.spab.org.uk/sites/default/files/Caring-for-old-floors_SPAB-help-guide%20(002).pdf)]

The Dutch guidance adds an especially useful material-economy detail: boards
were sometimes left tapered to follow the tree, then laid head-to-tail,
producing nonparallel seams. It also notes that quarter/rift-sawn narrow boards
shrink less across their width and wear better, but presents them as a distinct
floor type. This means controlled width and taper variation are historically
meaningful; randomized gaps and skew are not substitutes for construction.

For this project, broad local oak is a defensible default for a prosperous 1544
urban interior. Pine should be a supported variant, particularly where regional
supply or building status warrants it. Exact species distribution must remain a
world-generation decision rather than a texture-level assertion.

### Board direction, support, butt joints, and fasteners are one system

Floorboards are structural members. Their direction is constrained by the
supporting beams or joists, and short ends need support. Rijksdienst describes
direct nailing as the common attachment, while also documenting blind nailing,
dovetail-shaped wedges, laths, anchors, loose splines, tongue-and-groove
variants, and plain butt seams connected with dowels or iron pins. It explains
that connections between boards reduced dust fall-through, stiffened the floor,
and resisted warping. The chronology of every listed method is not specified on
the web page, so the 1544 default must not indiscriminately combine them.

The same source describes wide boards sometimes running along beams with their
long seams positioned above the beams; end joints could be covered by moulded
strips. It also notes that, where no separate ceiling existed, the boards formed
the ceiling of the room below, with surviving painted undersides known from the
sixteenth century. These are reminders that the floor assembly is visible
architecture, not only a top-facing material.

Transferable NPS documentation of historic flooring describes boards levelled at
the joists, undersides hacked with an adze to a controlled reference thickness,
and either face or edge nailing.
[[US National Park Service, *Notes on Historic Flooring*](https://www.nps.gov/crps/CRMJournal/CRMBulletin/v13n4.pdf)]
This is useful craft evidence but not direct German dating.

Practical consequences for the generator:

- Establish a room's structural span and joist axis first; board direction and
  permissible butt-joint stations follow it.
- A butt joint belongs over a support. It should expose actual end grain and a
  cross-grain cut/tool signature, not merely a dark transverse line through
  continuous longitudinal grain.
- Adjacent butt joints should not create an implausibly weak continuous seam.
  Long room-spanning boards may have no butt joint at all.
- Visible forged nails, if selected by the construction preset, sit at support
  crossings and have limited piece-to-piece variation. They must not become an
  evenly repeated decorative dot grid.
- A hidden-wedge, spline, or blind-fastened preset should not show the same
  face-nail pattern.
- The underside and edges require correct board direction if exposed around a
  stairwell, gallery, floor opening, or unceiled lower room.

### Surface preparation should read as hand work, not random damage

Riven, axed, pit-sawn, and planed surfaces leave different directional evidence.
SPAB emphasizes retaining original surface, tool marks, and patina, and warns
that machine sanding cuts away both and can cut across grain. Historic England's
archaeological recording guidance likewise treats saw, chisel, adze, auger,
joint, and fixing marks as diagnostic evidence.
[[Historic England, *Archaeological Recording Manual*](https://historicengland.org.uk/content/docs/research/historic-england-archaeological-recording-manual-2018)]

The top face of a usable interior floor nevertheless had to be sufficiently
level. Adze facets or plane scallops should be broad, shallow, directionally
coherent, and subordinated by later foot wear. Deep axe gouges across every
board would imply unfinished structural timber rather than a maintained walking
surface. Pit-saw kerfs are most plausible on sawn or hidden faces unless
deliberately retained; a planed face can show gentle longitudinal tracks and
slight local facets.

End grain is a separate anatomical surface. A butt joint should interrupt face
grain and reveal compact ring/fibre structure across the thickness. Texture-only
end grain can be a narrow semantic band, but visible floor edges at stairwells
and openings should use actual side/end material coordinates on geometry.

### Gaps, cupping, wear, dirt, and finish have causes

Rijksdienst says cracks and gaps belong to old floors and often remain
functional; it treats gaps above roughly 7 mm as candidates for inserting
matching wood, while warning that seasonal swelling can eject fillers or damage
boards. It identifies moisture as a cause of swelling, staining, rot risk, and
deformation, and heat/low humidity as a cause of shrinkage and cracks. It also
notes iron reacting with tannin-rich oak to make black staining. These are
causal relationships useful for PBR synthesis, not a command to make every newly
laid floor visibly ruined.

SPAB describes centuries of use, settlement, fading, repairs, and patches as
part of a floor's patina. It notes that wear and finish failure concentrate at
high-traffic locations such as doorways, that excessive wax can trap dirt and
darken a surface, and that highly polished or sealed surfaces were rare before
the nineteenth century. Traditional wax gives limited protection and a mild
lustre, not a uniform modern varnish.

Therefore:

- Edge gaps mainly reflect board construction, seasonal shrinkage, and age. Gap
  width should be coherent within a room state, with limited local deviation,
  rather than independent noise on every edge.
- Cupping should correlate with board width, ring orientation/cut mode, moisture
  history, and fixing. Neighboring texels on a board must share the same
  cross-board deformation.
- Long checks follow grain and are more plausible near board ends, fasteners,
  defects, or badly dried material.
- Dirt accumulates in open seams, depressions, thresholds, under furniture,
  along walls, and in low-traffic corners. Traffic removes loose dirt and
  polishes/abrades raised fibres along connected paths.
- Wear fields must be room semantic: door-to-door paths, hearth/work zones,
  stair approaches, and furniture clearances. The same sinusoidal central stripe
  repeated in every room is not evidence of human use.
- Wax/oil/soap/unfinished presets change color and roughness together. A lightly
  waxed traffic path may become darker and smoother; worn untreated oak can
  become smoother on high points while exposed damaged fibres remain rough.
- Iron-nail halos on oak should be rare, local black/tannin staining linked to
  actual fasteners and damp, not generic dark speckles.

## Procedural-art prior art

### Separate construction identity from wood anatomy

Adobe's official aged-plank course is organized as wood pattern, plank pattern,
height, knots, warped integration, nails, roughness, base color, and material
blending.
[[Adobe Substance 3D, *Creating Old Wood Planks in Substance 3D Designer*](https://www.adobe.com/learn/substance-3d-designer/web/creating-old-wood-planks-in-substance-3d-designer)]
This order is useful because board layout, wood anatomy, and ageing can remain
independently parameterized and then be causally combined.

A recent procedural aged-wood breakdown begins with a linear gradient for fibre
flow, shapes it with curves, applies restrained per-piece variation, and uses
directional warp so dents and later detail follow the grain.
[[Nichapat Thanasuan, *Procedural Aged Wood Material Breakdown*](https://www.artstation.com/blogs/sakijung/dzBaR/procedural-aged-wood-material-breakdown-substance-designer)]
Not Lonely's wood workflow similarly separates fibres, ageing masks, color,
normals, roughness, and controllable wear.
[[Not Lonely, *Procedural Wood Material*](https://www.not-lonely.com/blog/tutorials/procedural-wood-substance-designer/)]

The transferable graph is:

1. Generate a structural board map from room bounds, joist axis/stations,
   construction preset, and stable board IDs.
2. Give each board a wood-space transform: longitudinal axis, across-board
   coordinate, cut/ring mode, and end positions.
3. Generate anatomically coherent broad growth structure, rays/pores appropriate
   to species, restrained fibre relief, and true end-grain zones.
4. Generate construction height: board elevation, edge fit, cup, butt cuts,
   fastener depressions, and hand-tool surface.
5. Generate age/moisture/traffic/finish masks in room space.
6. Derive albedo, normal, AO, and roughness from those shared causes instead of
   layering unrelated noises.

The current recipe already approximates steps 1–4. Its largest opportunity is to
replace periodic generic traffic and per-board random damage with semantic room
fields and to bind the output at its declared physical scale.

### Stable per-board variation belongs in IDs or the shader, not duplicated geometry

A SideFX floor/plank discussion recommends preserving instancing and passing a
stable plank ID into the shader to randomize UV offsets instead of modifying
every packed primitive's attributes.
[[SideFX forum, *Randomise UVs on packed geo per copy*](https://www.sidefx.com/forum/topic/57139/)]
That maps well to the project's deterministic piece IDs: a limited library of
anatomically plausible wood phases can be indexed per board without unique
meshes or giant atlases.

Another SideFX thread diagnoses swimming wood patterns caused by deriving them
from changing surface parameters, recommending stable UVs or rest-space
coordinates established before deformation.
[[SideFX forum, *woodplank VEX shader*](https://www.sidefx.com/forum/topic/3533/)]
For Fabelgeist, room-local metric UVs should be authored when the floor assembly
is resolved and remain invariant under building placement. Rotating or
translating the building must not change which board lies at a doorway.

### Roughness must absorb unresolved relief

Valve's GDC 2015 VR rendering talk demonstrates geometric specular aliasing and
derives a roughness floor from normal derivatives/curvature.
[[Alex Vlachos, GDC 2015, *Advanced VR Rendering*](https://media.steampowered.com/apps/valve/2015/Alex_Vlachos_Advanced_VR_Rendering_GDC2015.pdf)]
The exact shader code need not be copied, but the principle applies directly:
high-frequency pores, saw marks, grain normals, and sharp gap bevels must not
disappear in a mip while retaining a narrow specular response.

Generate color mips in linear light. Decode, average, and renormalize
tangent-space normals, or derive each normal mip from a filtered height pyramid.
Increase perceptual roughness according to unresolved normal variance. Preserve
the mean coverage/darkness of structural gaps without allowing one-pixel black
seams to pulse as the camera moves.

## Room-local metric UV and semantic contract

For each room or continuous floor assembly, define:

- origin: a stable construction corner, not world origin;
- `V`: board/grain direction, ordinarily perpendicular to or structurally
  related to supporting joists;
- `U`: across-board direction;
- units: metres, with recipe values interpreted directly;
- joist phase: room/assembly metadata shared by butt joints, face nails, and
  visible support geometry;
- board IDs: stable under unrelated plan changes where feasible;
- semantic masks: entrances, circulation graph, hearth/work areas, wall
  perimeter, leaks/damp, furniture and repair zones.

Do not restart the board pattern independently per floor triangle or cell.
Adjacent coplanar room floor pieces belonging to one assembly must share origin
and basis. Conversely, distinct rooms may intentionally change direction, board
set, condition, or threshold detail. A stairwell or opening must clip boards
geometrically without shifting the surrounding field.

If the continuous floor exceeds the recipe's periodic 7.2 m domain, avoid an
obvious identical room-sized square. Combine the periodic fine material with
nonperiodic assembly-level board IDs and semantic weather/wear masks, or use
multiple deterministic tile transforms that preserve board direction and joint
support. Random 90-degree rotations are invalid because they rotate grain and
support logic.

## Geometry versus texture and LOD

Texture/normal/height should own:

- the broad board field and restrained width/taper variation;
- shallow edge fit and gaps;
- grain, pores, rays, subtle tool facets, checks, nail depressions/stains;
- finish, traffic polish, dirt, damp, and repair color/roughness.

Geometry should own:

- stairwell, gallery, hatch, and broken-floor edges where board thickness and
  end/side grain are visible;
- thresholds, transition strips, moulded seam covers, and major patches;
- genuinely lifted, missing, or severely cupped hero boards;
- the floor's relation to joists where the underside is exposed.

LOD0 can retain true edge thickness, thresholds, major patches, and a small
number of lifted pieces. LOD1 can use a continuous plane with full board/gap
normal and semantic wear. LOD2 should retain only stable board cadence, broad
color/roughness variation, and circulation mass; pores, nails, tiny checks, and
narrow tool marks should be gone. Interior floors often disappear through
occlusion before a city-distance LOD matters, but large halls, open galleries,
and destroyed structures still require stable distance behavior.

## Concrete recommendations

1. **Bind the actual generated material.** Replace the tactical checker for
   `BuildingLodMaterial::Floor` with the `plank_floor` albedo, normal, and ARM
   set before judging the recipe visually.
2. **Replace the generic 2 m world projection.** Author room-local metre UVs and
   sample the declared 7.2 m domain. Keep board direction stable under building
   rotation and placement.
3. **Make the floor assembly explicit.** Supply room bounds, structural span,
   joist axis/spacing, opening polygons, and construction preset. Butt joints
   and fasteners must be generated from this assembly rather than inferred from
   a globally repeating texture alone.
4. **Default to broad oak, with variants.** Keep widths centered near the
   current ~0.33 m and allow a controlled upper tail toward the historical
   comparison's 0.45 m or more. Add optional head-to-tail taper. Support pine
   and alternate fixing/finish presets without claiming an unsupported universal
   distribution.
5. **Allow full-length boards.** Do not force one periodic butt joint into every
   board. When a butt joint is required, place it over a support, add actual end
   grain, and avoid aligned neighboring joints.
6. **Refine tool evidence.** Keep broad low-amplitude planing/adze facets and
   longitudinal irregularity. Avoid uniform cross-board scallops, excessive
   gouges, or modern perfectly sanded smoothness.
7. **Make deformation causal.** Correlate cup and check with width, cut/ring
   mode, moisture, age, and fixings. Gaps should share a room-level shrinkage
   component plus small board deviations.
8. **Replace the periodic traffic stripe.** Drive wear, polish, and dirt with
   navigable room paths and semantic zones. Doorways, stairs, hearths, and work
   areas should tell a coherent use story.
9. **Correlate channels.** Joint depth causes contact AO and dirt retention;
   worn high points become smoother; damp changes albedo and roughness; iron
   nail plus moisture may blacken oak locally. Do not bake broad AO into albedo
   or darken every seam equally.
10. **Correct mip generation.** Use linear-light albedo filtering, normalized
    normal mips, variance-aware roughness, and distance tests for
    gaps/nails/grain.
11. **Resolve exposed edges in geometry.** Stair openings and damaged floors
    must show thickness and correct side/end grain; the surface texture cannot
    plausibly cap a vertical cut.
12. **Keep flooring socially and architecturally varied.** Upper floors,
    prosperous rooms, workshops, lofts, and humble ground floors need not share
    one finish or even one floor material. That selection belongs to building
    generation.

## Acceptance tests

### Determinism and construction

- Same seed and assembly inputs produce byte-identical maps and board IDs.
- Moving or rotating a building preserves the material relative to its rooms.
- Coplanar pieces in one floor assembly share UV origin/basis with no seam or
  phase reset.
- Rendered board widths, gap widths, and joist intervals match declared metres
  at multiple room sizes and rotations.
- Every butt joint and face nail lies over a valid supporting joist station.
- Adjacent butt joints do not form forbidden continuous seams; full-length
  boards are possible.
- Butt ends display cross-grain/end-grain structure rather than continuous face
  grain.
- Openings clip boards without covering stairwells or shifting neighboring UVs.
- Exposed edges distinguish face, side, and end grain.

### Distribution and causal relationships

- Width/taper, cut mode, tool marks, checks, cup, nails, repairs, and gaps stay
  within preset-specific bounds.
- Cup magnitude correlates statistically with width/cut/moisture inputs.
- Checks align longitudinally and concentrate near ends, fasteners, or defects.
- Wear follows fixture navigation paths and concentrates at
  entrances/stairs/work zones; changing the room graph changes wear coherently.
- Loose dirt is depleted on traffic paths and accumulates in
  gaps/perimeters/obstructions.
- Nail staining only occurs with compatible species, exposed iron, and moisture.
- Metallic remains zero for wood pixels; nail metal, if separately represented,
  is tightly masked.

### Mips, LOD, and visual review

- Albedo mip values match a linear-light reference downsample.
- Decoded normal mip vectors remain unit length within tolerance.
- Effective roughness does not decrease as grain/tool relief becomes unresolved.
- Gap coverage and mean darkness converge smoothly; no black seam flicker or
  disappearing/reappearing nails occurs in a moving-camera distance sweep.
- LOD transitions preserve board direction, scale, broad tone, and traffic
  pattern.

Capture fixed-seed rooms under diffuse daylight and low grazing light: a new
broad-oak floor, a lightly waxed prosperous room, a worn circulation room, a
damp/neglected room, a pine variant, a stairwell edge, an exposed underside, and
a repaired floor. Include overhead orthographic, eye-level oblique, doorway
approach, and LOD distance sweeps. Reject barcode grain, modern narrow strips,
universal glossy varnish, decorative nail grids, joints unsupported by
structure, isolated random dirt, overly black gaps, repeated identical traffic
bands, and geometry whose visible cut faces reuse top-face grain.

## Evidence, inference, and project decisions

**Evidence:** Northwestern European heritage sources support visible structural
board floors, medieval oak, late-fifteenth-century pine examples, broad and
variable boards, riven/axed/pit-sawn manufacture, tapered head-to-tail laying,
multiple fastening/joint systems, causal shrinkage gaps and moisture
deformation, restrained traditional finishes, and traffic-related patina.
Practitioner sources support stable piece IDs, fixed/rest-space material
coordinates, separated board/anatomy/wear graphs, directional warping, and
roughness treatment for unresolved normal variation.

**Inference:** Broad oak boards with restrained hand-working are a better
general 1544 German urban-interior default than narrow uniform planks, parquet,
or aggressively distressed boards. A room-spanning or structurally joist-aligned
layout is more plausible than a decorative random stagger. Exact German regional
species, board dimensions, joint chronology, finish, and wealth distribution
require location- and building-specific evidence.

**Repository decisions:** The 7.2 m periodic domain, current board/joist counts,
room-local origin convention, construction preset API, traffic-mask source,
edge-geometry budget, and LOD thresholds are engine choices. Historical sources
do not mandate them. The highest-value first correction is to bind the generated
maps at declared metric scale with an assembly-local structural basis; otherwise
further grain and wear refinement will be assessed on the wrong board dimensions
and in the wrong rooms.
