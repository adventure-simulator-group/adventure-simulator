# Handmade brick procedural texture prior art

## Scope

This report concerns exactly the `HandmadeBrick` procedural surface for
early-modern German brickwork. It covers historically plausible brick formats
and bonds, moulding and firing variation, irregular edges and warped faces,
lime mortar, wear and deposits, causal PBR channels, physical scale, façade
UVs, repetition, mip behavior, and LOD use.

It does not claim that Germany in 1544 had one national brick size, bond, clay
color, or joint finish. Those varied by region, date, building status, brick
source, wall thickness, and whether brick formed a full wall, a facing, or
fachwerk infill. The correct goal is a parameterized historic masonry family,
not one texture universally labeled “old brick.”

## Repository facts and constraints

The following are facts observed in this worktree, not claims from external
sources.

- `HandmadeBrick` produces albedo, OpenGL normal, height, and ARM maps. Its 512
  by 512 tile represents 2.4 metres square, about 4.688 mm per source texel,
  with a declared 14 mm full height range.
- The tile has 30 courses and 10 nominal brick lengths per course. After 14 mm
  joints, each visible stretcher is about 226 mm long and 66 mm high.
- The layout is a running stretcher bond with every other course offset by half
  a brick. There are no headers, closers, corners, arches, sills, decorative
  bonds, or bond variants.
- Every brick receives deterministic centre, width, and height variation;
  bowed and occasionally chipped edges; low-amplitude face noise; broad cup;
  and twist. The tile contains 300 stable brick identities, which is a useful
  base repeat size.
- Brick albedo selects one of five red-brown colors plus a very small face-noise
  shift. Mortar selects one of two gray-beige colors. There is no within-brick
  firing gradient, soot, damp, limewash, efflorescence, repair, or exposed-core
  distinction.
- Mortar height is a low periodic sine field. Joint profile and tooling are not
  modeled explicitly, and mortar color/roughness are partially driven by the
  nearest brick's `face_noise`, not by mortar aggregate or age.
- Brick roughness is high and increases near edges and with face-noise
  magnitude. Mortar roughness is fixed at about 0.925. Metalness is zero. AO
  comes from multi-distance height comparisons.
- Tests prove deterministic output, approximate height continuity at tile
  edges, half-brick course staggering, current nominal dimensions and joint
  widths, 300 brick identities, a minority of chipped edges, near-planar
  interiors, channel variation/nonmetalness, sRGB albedo, and complete mip
  dimensions.
- The shared mip helper averages encoded bytes. It does not downsample sRGB
  albedo in linear light, decode/filter/renormalize normals, convert unresolved
  normal variance to roughness, or preserve mortar/brick coverage explicitly.
- High-detail and LOD façade UVs repeat every 2 metres in wall-tangent and
  vertical directions, while this texture declares a 2.4 metre tile. Binding it
  without a UV transform makes dimensions 20 percent too small.
- `WallStyle::Brick` maps to `WallMaterialClass::CivilianMasonry`, whose audit
  accepts wall thicknesses of 0.40–0.70 metres. Tactical presentation also uses
  `CivilianMasonry` for brick-infill appearances and may substitute a fully
  rendered finish. One material class therefore spans structurally different
  uses.
- Tactical brick currently uses a 64-pixel palette brick pattern, not the
  generated `HandmadeBrick` maps. The procedural lab can display the recipe,
  but city screenshots do not yet prove its façade result.
- LOD2's separate `FachwerkBaked` generator has its own simple brick pattern
  for brick-infill appearances and does not inherit this recipe's dimensions,
  bond, mortar, or color statistics.

These facts expose two prerequisites: masonry needs a regional/use-specific
format-and-bond contract, and the runtime material must honor the recipe's
physical scale.

## Historical and conservation evidence

### Brick dimensions were regional and period-specific

In northern German and Baltic Brick Gothic, the so-called monastic format was
common for representative masonry: roughly 280 × 150 × 90 mm to 300 × 140 ×
100 mm, with joints around 15 mm
([BauNetz Wissen, *Klosterformat*](https://www.baunetzwissen.de/glossar/k/klosterformat-1326607);
[Hanseatic League cultural heritage report](https://kulturland.se/wp-content/uploads/2020/06/Hanseatic-league-english-version.pdf)).
A Hanseatic heritage survey gives earlier regional ranges of roughly 240–290
mm length, 120–135 mm width, and 75–110 mm height and notes later gradual
reduction rather than one fixed standard
([same Hanseatic heritage report](https://kulturland.se/wp-content/uploads/2020/06/Hanseatic-league-english-version.pdf)).
An independent historic-building association's measured hand-struck examples
range from 280 × 140 × 80 mm down through several shorter and thinner regional
formats
([IG Baupflege Nordfriesland & Dithmarschen, *Historische Handstrichziegel*](https://www.igbaupflege.de/de/lexikon/mauerwerk/einfuehrung.php)).

The later German `Reichsformat` is not a safe 1544 default: its standardization
belongs to the nineteenth century. A superficially similar 250 × 120 × 65 mm
brick may exist in older regional contexts, but should not be justified by the
later national standard.

**Inference for `HandmadeBrick`:** the current 226 × 66 mm face is smaller and
shallower than the well-attested northern monastic format. It may suit a
smaller regional brick or thin fachwerk infill, but the recipe needs named
presets rather than a universal “early-modern” test range. At minimum provide a
northern monastic/Hanseatic preset and a smaller regional preset, each with
source-backed ranges. Building geography and role should select them
deterministically.

At 4.688 mm per source texel, current maps can describe centimetre-scale edge
deformation, sand drag, chips, warped faces, and mortar tooling. They cannot
resolve individual fine pores robustly; those should be restrained
albedo/roughness statistics that vanish in mips.

### Bond is structural information, not decorative staggering

Historic northern brick architecture used multiple bonds, including Gothic
and monk/flying bonds; irregular hand-made monastic bricks influenced bond
design. The Hanseatic report specifically notes Gothic bond and the inclusion
of over-fired brick in façades. A computational heritage study models historic
brick walls through explicit parametric shape rules plus measured stochastic
variation rather than treating the bond as noise
([Tuncer et al., *Symmetry and Variance: Generative Parametric Modelling of Historical Brick Wall Patterns*](https://arxiv.org/abs/2210.12856)).

**Inference:** choose a bond from wall construction, not just appearance.

- A thin fachwerk infill leaf can plausibly show stretchers, but must terminate
  correctly against its bay and opening edges.
- A 0.40–0.70 metre solid masonry wall should not universally present an
  endless half-brick-thick running bond. Header courses or header/stretcher
  patterns should communicate wall bonding.
- Corners, jambs, arches, and wall ends require geometry-aware brick layout and
  closers; a globally repeated flat pattern cannot resolve them.
- Large church, fortification, and civic façades may use shaped, glazed,
  decorative, or regional bond modules and should not inherit a domestic
  stretcher tile by fallback.

The cheap near-term solution is a small library of physically parameterized
bond tiles selected by masonry role. The robust solution is façade-run brick
coordinates with explicit course and unit identities, allowing openings and
corners to terminate coherently.

SideFX brick-wall workflows reinforce the value of unit identity. A SideFX
forum example varies rotation and a color ramp per brick using the brick ID
rather than applying undifferentiated noise to the wall
([SideFX forum, *Procedural Brick Shader*](https://www.sidefx.com/forum/topic/56065/)).
Another practitioner workflow constructs bricks and mortar together rather
than treating mortar as empty background
([SideFX forum, *Bricks & Mortar*](https://www.sidefx.com/forum/topic/73689/)).
These are implementation precedents, not historical evidence.

GDC material-production evidence is directly relevant to the runtime treatment,
although it is not evidence for historic masonry. Volition's *Agents of Mayhem*
material talk uses brick as its concrete tiling example: small custom-colored
textures produced conspicuous repeated dark-brick landmarks, while removing all
such landmarks made the material bland. The production solution separated a
tintable base from restrained retained color variation, and treated brick as a
dielectric whose base reflectivity could usually be a scalar rather than a
dedicated high-frequency texture
([James Taylor, GDC 2017, *Agents of Mayhem: The Materials of Mayhem*](https://media.gdcvault.com/gdc2017/Presentations/Taylor_James_Agents_of_Mayhem.pdf)).
The transferable lesson is to preserve bounded unit-scale firing variation but
keep its most recognizable landmarks out of the short repeat; larger batch and
weather variation should come from a separate coarse control field.

Ubisoft's GDC destruction presentation also shows a brick motif as a continuous,
tileable UV-space pattern transformed into surface space. It distinguishes the
surface motif from feature-bound geometry and notes that actual geometry is more
expensive but survives cutting and can protrude
([Julien L'Heureux, GDC 2016, *The Art of Destruction in Rainbow Six: Siege*](https://media.gdcvault.com/gdc2016/Presentations/LHeureux_Julien_Art_Of_Destruction.pdf)).
For this recipe, that supports keeping the repeating brick/mortar field in
metric facade coordinates while assigning corners, jambs, arches, broken
edges, and strongly projecting units to geometry. It does not justify any
particular historical bond, brick size, color, or weathering pattern.

### Handmade character begins with manufacture and firing

Historic-building guidance attributes misshapen, under-fired, and over-fired
bricks to material preparation, pressure in the mould, and inconsistent firing
heat. It also emphasizes that mortar-joint character materially changes the
appearance of the wall
([New Forest National Park Authority, *Brickwork*](https://www.newforestnpa.gov.uk/document/brickwork/)).
Conservation practitioners describe firing position as a major color source:
bricks nearest greater heat can become substantially darker or blackened
([Paul Ashton Architects, *Guide to handmade bricks and conservation lime mortars*](https://www.paulashtonarchitects.com/blog/2025/3/4/guide-hand-made-bricks-amp-conservation-lime-mortars)).
The Hanseatic report's note about deliberately included over-fired bricks is
useful period evidence for restrained dark units in northern work.

Procedural material practice commonly starts from a stable brick mask, gives
each unit its own random values, then warps/chips unit edges and layers surface
detail. Jussi Jantunen's documented Substance workflow uses warp and slope-blur
operations to remove large pieces from old brick edges
([Jantunen, *Creating procedural textures for games*](https://www.theseus.fi/bitstream/10024/132562/1/Jantunen_Jussi.pdf)).

**Inference:** variation should exist at several causal scales:

- per batch/kiln: clay family and average firing;
- per brick: under-fired, ordinary, hot/over-fired, inclusion-rich, or
  occasional glazed/shaped category where historically selected;
- across one brick: face skin, mould/sand drag, firing gradient, broad cup or
  twist, and local inclusions;
- at edges: rounded arrises, mould deformation, sparse chips, and exposed
  softer core where damage breaks the fired skin.

The current five discrete red-brown colors are a reasonable placeholder, but
the tiny ±3 byte mineral shift does not create within-brick causality. Add
bounded low-frequency gradients and firing/mineral masks shared by albedo and
roughness. Avoid independent rainbow variation or equal-probability black
bricks. Chipping should expose a coherent core color and roughness, not merely
shrink the silhouette while leaving the same face material.

### Mortar is a visible material with a profile and life cycle

Historic masonry used compatible lime mortar, and its aggregate, color, and
joint finish are part of the wall's appearance. Building Conservation notes
that historic pointing treatments reflect long-developed skills and that the
original aggregate, mix, profile, and color should be matched
([Parker, *Joint Finishes on Historic Brickwork*](https://www.buildingconservation.com/articles/_gsdata_/_saved_/brickwork-joint-finishes/brickwork-joint-finishes.htm)).
The National Park Service explains that softer historic masonry and hard modern
mortar are a damaging combination, and that salts transported by water can
crystallize as efflorescence
([NPS, *Mortar, Unsung Hero of History*](https://home.nps.gov/articles/000/mortar-unsung-hero-of-history.htm)).

**Inference:** mortar needs its own procedural field and parameters:

- lime/sand color derived from local aggregate, not nearest-brick noise;
- a selectable flush, slightly recessed, weathered, or historically supported
  tooled profile;
- bed and head joints with related but non-identical width variation;
- trowel smears, small aggregate, shrinkage, erosion, and rare repair sections;
- height interaction with warped brick edges so mortar fills gaps rather than
  forming an unrelated sine wave below every brick.

At close range, the topography should be dominated by the brick-to-joint step,
warped arrises, joint profile, and sparse damage. Micro-noise should not carry
the same amplitude as those structural features. The 14 mm declared relief is
appropriate for recessed joints and meaningful chips, but ordinary face
granularity should use a small fraction of it.

### Soot, damp, salts, and wear belong to the building

The National Park Service identifies water as a principal cause of historic
brick deterioration and distinguishes soot/smoke from other stains when
assessing masonry
([NPS, *Common Problems with Brick Masonry*](https://www.nps.gov/articles/common-problems-with-brick-masonry.htm);
[NPS, *Assessing Cleaning and Water-Repellent Treatments*](https://www.nps.gov/orgs/1739/upload/preservation-brief-01-cleaning-masonry.pdf)).
Water movement also causes salt blooms and freeze-thaw deterioration.

**Inference:** these are not stationary 2.4 metre tile details.

- rising damp and salt deposits depend on ground height and drainage;
- rain streaking depends on sills, copings, gutters, roof edges, projections,
  and façade orientation;
- soot concentrates around chimneys, openings, sheltered recesses, streets,
  and combustion sources;
- splash erosion and green growth concentrate near the base and persistent
  wetness;
- repairs occupy bounded regions and can change mortar or brick batches.

Generate them as building/world-space masks layered over a comparatively clean
base material. If the intended 1544 city is maintained and inhabited, use
restrained probabilities; age does not imply uniform ruin or black soot.

## Channel relationships

Brick and lime mortar are dielectrics, so metalness remains zero. The PBR maps
should share causes without becoming copies.

- Fired-skin color and mineral distribution influence albedo; roughness may
  respond subtly, while height changes only where inclusions or manufacturing
  marks are large enough.
- Under-fired porous units should generally be rougher and more moisture-prone;
  hot/over-fired faces may be darker and locally denser/smoother, within the
  selected reference family.
- Warping changes height and normals but should not automatically darken
  albedo.
- Chips remove the fired skin, lower height, expose a differently colored,
  rougher core, and receive local AO only at their recessed edges.
- Mortar recession and erosion drive height/normal/AO; mortar aggregate drives
  albedo and roughness at resolvable scales.
- Wetness darkens albedo and commonly lowers roughness, but only under an
  exterior moisture mask. Efflorescence lightens albedo and may add powdery
  roughness with little height.

Avoid painting every joint black. AO should emerge from joint depth and local
occlusion, then weaken appropriately at distance. Albedo should remain usable
under varied lighting rather than containing baked directional shadow.

## Physical UVs and façade use

Brick courses require stable façade coordinates. U follows accumulated distance
along a continuous wall run; V follows height in metres. Use the recipe's
declared scale, not normalized per-wall UVs. Courses should remain level across
continuous sections, turn coherently at corners, and meet openings through
explicit jamb/arch rules rather than being clipped arbitrarily.

For fachwerk brick infill, coordinates should be panel-local so units terminate
inside each timber bay. A 2.4 metre global tile continuing beneath posts can
show impossible half-bricks on both sides of a frame. For full masonry, run-
local coordinates should preserve bond across ordinary façade segments, while
corners and openings introduce deterministic bond modules.

The schema should distinguish at least:

- brick fachwerk infill;
- full civilian load-bearing brick masonry;
- monumental/fortified brick masonry;
- rendered brick substrate, where brick should not be visible except at
  localized finish loss.

This should be a clean final material contract rather than inference from one
overloaded `CivilianMasonry` class or a compatibility fallback.

## Tiling, mips, distance, and LODs

Three hundred unit identities provide good local diversity, but periodic
course/color patterns can still reveal the 2.4 metre square at city scale.
Preserve exact bond while breaking appearance repetition with deterministic
batch-scale color fields, a small set of compatible Wang-like variants, or
per-façade unit seeds. A published Wang-tile approach demonstrates how wall
patterns can extend stochastically while controlling long repeated lines
([Kopf et al., *Procedural Wang Tile Algorithm for Stochastic Wall Patterns*](https://arxiv.org/abs/1706.03950)).
Use that as a technical option, not a mandate; bond correctness has priority
over randomization.

Semantic mip generation should:

- downsample albedo in linear light and re-encode to sRGB;
- decode, filter, and renormalize normals;
- transfer unresolved normal variance into roughness;
- preserve average brick/mortar coverage and prevent thin joints from
  flickering or disappearing asymmetrically;
- filter height without making isolated deep chips lower a whole coarse texel;
- reduce AO as joint detail becomes unresolved rather than turning distant
  walls into dark grids.

At LOD0, brick faces, profiles, major chips, special units, and opening/corner
bond can remain. LOD1 should preserve exact course/bond rhythm and major color
units but reduce fine surface relief. LOD2 should bake the bond, brick/mortar
coverage, and low-frequency firing variation into a stable façade; tiny chips,
pores, and deep black joints should disappear. Every LOD should retain average
tone and course alignment to avoid crawling or obvious material swaps.

## Recommended implementation sequence

### 1. Define format, bond, and wall-role presets

- Add named, sourced regional brick-format ranges rather than one universal
  dimension assertion.
- Separate thin infill, full civilian masonry, and monumental masonry.
- Select a plausible bond for each role and add deterministic header/stretcher
  layouts where wall thickness requires them.
- Reconcile the 2.4 metre recipe tile with the mesh's 2 metre repeat.

### 2. Bind and review the real generated material

- Show `HandmadeBrick` on neutral test walls and actual façades, not only in a
  square material preview.
- Replace or deliberately composite the tactical 64-pixel brick placeholder.
- Define how building palette variation tints kiln/clay parameters without
  destroying channel relationships.

### 3. Rebuild units from manufacture

- Keep explicit brick IDs and derive size, mould distortion, cup/twist, fired
  skin, core, edge shape, and firing category from each ID.
- Add within-brick low-frequency firing/mineral structure.
- Couple chips to exposed core and reserve large damage for façade overlays.

### 4. Give mortar its own generator

- Generate aggregate/color independently from brick faces.
- Parameterize joint width and profile by masonry preset.
- Fill the actual irregular gap between units, including controlled smears,
  erosion, and repairs.

### 5. Add building-space weathering and semantic mips

- Drive damp, efflorescence, soot, splash, and runoff from the building and
  environment.
- Correct albedo/normal filtering, compensate roughness, and preserve bond and
  joint coverage through LODs.
- Feed the same dimensions, bond, and average statistics into the LOD2 bake.

## Acceptance and regression tests

### Deterministic numeric tests

- Assert every preset's length, width, height, and joint distributions in
  metres, including runtime UV scaling.
- Prove bond sequences, half-lap offsets, header/stretcher ratios, and periodic
  closure for each tile or façade module.
- Verify courses remain level and continuous across triangulated façade runs.
- Verify corner/opening modules do not create overlapping units or impossible
  slivers below a minimum physical size.
- Measure per-unit bow, twist, chip coverage, arris radius, face relief, mortar
  recession, and core exposure in millimetres.
- Prove firing categories and color fields are deterministic, spatially
  bounded, and do not alter brick geometry or scale.
- Prove mortar parameters are independent of nearest-brick face noise.
- Verify metalness is zero and causal roughness/albedo/height correlations stay
  within reviewed bounds.
- Compare albedo and normal mips against linear-light and decoded-vector
  references; test mortar coverage and deterministic LOD stability.

### Visual fixtures

Review under neutral, overcast, and grazing light:

- a measured grid with one brick and joint annotated in centimetres;
- north-German monastic and smaller regional format walls side by side;
- thin stretcher-bond infill, full bonded masonry with headers, a corner, a
  jamb, an arch, and a rendered-brick damage sample;
- clean, under-fired, ordinary, and sparse over-fired unit examples;
- flush and weathered lime-mortar profiles;
- a long façade and 2 by 2 tile view to expose repeated color/chip fields;
- restrained building-space rising damp, sill runoff, soot, efflorescence, and
  repair masks one at a time;
- matching LOD0/LOD1/LOD2 views at stationary and slow-moving transitions.

An independent visual reviewer should reject uniform modern-sized bricks,
stretcher bond on every thick wall, courses that drift at mesh seams, mortar
derived from brick noise, inflated pillow-like faces, equal chips on every
arris, arbitrary per-pixel color speckle, black painted joints, repeated dark
bricks in a square grid, soot/damp repeated per tile, shimmering mortar, or an
LOD transition that changes course scale or average wall color.

## Evidence, inference, and project decisions

- **Evidence:** sixteenth-century German brick was regionally variable; large
  monastic-format bricks and approximately 15 mm joints are well attested in
  northern/Baltic representative work; historic bonds included more than
  running stretchers; hand moulding and uneven firing create shape and color
  variation; lime mortar profile materially affects appearance; water, soot,
  and salt deposits have distinct causes.
- **Inference:** the best affordable system is a small set of regional and
  structural presets, explicit unit IDs and bonds, manufacture-derived unit
  variation, independent lime mortar, and building-space weathering. The
  current 226 × 66 mm running-bond tile may remain as one preset but should not
  define all 1544 German brickwork.
- **Repository decisions still required:** geographic selection rules; exact
  preset references; which wall roles expose brick; bond handling at corners
  and openings; palette-to-clay parameter mapping; tactical material binding;
  and how far LOD façades consume the same bond statistics.

The minimum credible milestone is a real-scale comparison fixture where a
thin brick-infill bay and a thick bonded wall use deliberately different but
sourced format/bond presets, mortar fills their actual irregular joints, firing
variation remains restrained and causal, and both converge without scale or
color pops through the LOD chain.
