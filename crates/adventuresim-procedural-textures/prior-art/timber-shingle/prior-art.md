# TimberShingle prior art

## Scope and confidence

This is an internal implementation report for the `TimberShingle` procedural
texture. It asks what a plausible wood-shingle roof in and around a German city
in 1544 should look like, and which production techniques transfer well to the
repository's deterministic CPU texture generator.

The historical evidence supports several distinct shingle systems rather than
one universal material. It supports split wood, strong lap, grain-following
manufacture, and regionally variable species and fixing methods. It does **not**
support treating every roof as a field of randomly battered, heavily cupped
modern “rustic shakes.” Measurements from modern preservation sources are useful
for material physics and craft practice, but are not by themselves proof of
exact sixteenth-century German dimensions.

The procedural-art sources below mostly describe transferable construction
methods rather than period history. The recommendations label historical
evidence, practitioner evidence, inference, and repository constraints
separately.

## Repository baseline

The current recipe is already more structured than a generic wood-noise texture:

- It creates a 3.2 m periodic field at 512 px, with sixteen courses and eighteen
  shingles per course. That implies approximately 200 mm visible course exposure
  and 178 mm nominal shingle width.
- Stable piece IDs, shared boundary jitter, alternating course offsets, and
  restrained lean/wander create coherent rather than per-pixel variation.
- Each piece has cup, twist, thickness, a tail lip, recessed undercourse
  contact, sparse longitudinal split fibres, and a limited tail-shape
  vocabulary. Most tails are square; taper and notch are minorities.
- Height drives normal and contact response; albedo has per-piece brown
  variation and a grey weathering term; roughness is generally high.
- Tests cover determinism, periodic boundaries, metric scale, directional
  structure, width distribution, sparse fibres, tail classes, checks, contact,
  channel validity, and mip generation.

Important integration constraints are currently outside the recipe:

- Building roof UVs use a generic world-XZ projection with a 2.0 m repeat, not a
  slope-local metric basis. If the 3.2 m recipe is sampled as a 2.0 m repeat,
  the intended 178 mm width becomes about 111 mm and the 200 mm exposure about
  125 mm.
- Tactical `RoofMaterial::TimberShingle` currently falls through to the generic
  timber-roof presentation rather than binding this generated map set.
- The roof mesh is a broad planar prism. It has no shingle-specific eave, verge,
  ridge, hip, valley, or repair geometry.
- Shared mip generation averages encoded RGBA bytes. That is not correct
  filtering for sRGB color or tangent-space normals, and roughness is not
  adjusted for unresolved normal variance.

Those are repository facts, not conclusions from the cited sources.

## Historical construction evidence

### Split and sawn are materially different

Grabner, Nemestothy, and Wächter's comparative study is the strongest directly
relevant source. It reports that wooden roof coverings were traditionally made
by splitting, and that a split surface follows the wood's fibre structure. Split
shingles retain better flexibility and strength, cup less, and expose fewer cut
fibre ends to moisture than sawn boards. Durability falls when boards contain
juvenile wood, knots, or substantial fibre deviation. Their study also documents
both tangentially split larch shingles and radially split spruce/fir forms, and
describes double- and triple-covered roofs.
[[Grabner et al., *Wooden Roofing: Split Shingles versus Sawn Boards* (2022)](https://doi.org/10.1163/27723194-bja10002)]

This evidence argues for an explicit split/sawn construction parameter rather
than a generic “wood” appearance. For the default 1544 material, split and
subsequently dressed wood is the safer choice. A split face should have
directional, low-amplitude relief that follows the shingle; it should not
resemble a bandsaw-cut plank or isotropic bark.

The same study makes the selection logic important: straight-grained, knot-free
portions away from the pith are preferred because splitting follows fibres.
Therefore a roof assembled from sound new shingles should be visually quieter
than ordinary construction lumber. Knots and severe cross-grain defects should
be rare damage or low-grade variants, not evenly distributed decoration.

### Shape, lap, and fixing are systems, not loose variation

The literature distinguishes multiple traditions. Grabner et al. describe long,
thick `Legschindel` secured by overlying poles and stones as well as shorter
nailed systems; their reconstructed/tested three-layer arrangement uses 60 cm
pieces with roughly 20 cm exposure. This is not evidence that all German roofs
used exactly those dimensions, but it establishes that visible exposure can be
only a fraction of full length and that the hidden lap is structurally
meaningful.

The Landschaftsverband Westfalen-Lippe describes oak and larch shingles fixed to
laths with small iron nails, with rows heavily overlapping in double or triple
coverage. It also characterizes wood shingles as lighter and cheaper than
ceramic or stone roofing.
[[LWL-Denkmalpflege, *Holzschindeln*](https://www.lwl-dlbw.de/de/denkmaeler-entdecken/baugeschichten/18-holzschindeln/)]
A Deutsche Stiftung Denkmalschutz discussion of historic roof frames likewise
associates sparse, widely spaced rafters with light coverings such as straw or
wooden shingles, in contrast with heavier tile, metal, or stone.
[[Deutsche Stiftung Denkmalschutz, *Sparrenabstände*](https://www.denkmalschutz.de/denkmale-erhalten/kulturspur-2022/forschungsmethoden-und-spuren/sparrenabstaende.html)]

The practical visual consequences are:

- A course cannot be treated as independent decorative rectangles. Its joints
  must be covered by the course or courses above.
- Course offset may vary, but it must preserve drainage and joint coverage.
  Random alignment that creates uninterrupted vertical seams is a construction
  error.
- Most fasteners in a properly lapped field are concealed. Visible nail heads
  should signal a special system, a repair, a slipped shingle, or an exposed
  boundary rather than appear at every tile center.
- Long weighted shingles with roof poles are a separate geometry/material
  preset, not an extreme value of the current short-shingle noise distribution.

Experimental archaeology at Museumsdorf Düppel is useful, although its report
explicitly warns that archaeological roof evidence is sparse and reconstructions
require judgment. It chose split oak shingles about 40 cm long, attached with
wooden nails, and notes evidence that multiple roof-covering types coexisted
according to material availability and building function. It also cites later
early-modern Berlin fire regulations as evidence that wood-shingled buildings
remained numerous enough to regulate.
[[Museumsdorf Düppel Journal 2019](https://www.dueppel.de/wp-content/uploads/2022/01/DUePPEL_JOURNAL_2019_web.pdf)]
This supports roofscape variety, not a claim that a particular 1544 city had a
known percentage of wood roofs.

### A restrained surface is more authentic than exaggerated roughness

The US National Park Service preservation brief is later and geographically
different, so it is evidence for timber behavior and traditional hand
manufacture, not direct evidence for 1544 Germany. It records that handsplit
shingles were commonly dressed with a drawknife so they would lie flat and shed
water, and explicitly warns that making replacement shakes more irregular does
not make them more authentic. It also describes doubled starter courses and the
importance of correctly resolved eaves, rakes, ridges, hips, valleys, dormers,
and chimney junctions.
[[NPS Preservation Brief 19, *The Repair and Replacement of Historic Wooden Shingle Roofs*](https://www.nps.gov/orgs/1739/upload/preservation-brief-19-wood-shingle-roofs.pdf)]

This strongly supports the current recipe's quiet majority, sparse fibres, and
minority tail variants. It argues against increasing random height, raggedness,
cup, or notches merely to make the material “read.” Form should become more
legible through coherent lap shadows, correct scale, directional grain, and
boundary silhouettes.

### Checks, cup, and weathering have causes

Wood checks and warping are consequences of fibre direction, ring orientation,
width, defects, and moisture cycling. Oregon State University's maintenance
guide describes repeated wet/dry cycles as a driver of checking, splitting,
cupping, and warping.
[[Oregon State University Extension, *Care and maintenance of wood shingle and shake roofs*](https://extension.oregonstate.edu/catalog/pnw-733-care-maintenance-wood-shingle-shake-roofs)]
Grabner et al. similarly connect sawn fibre damage and defects with moisture
uptake and decay.

The NPS brief reports that untreated shingles can weather toward silver grey or
soft brown depending on species and exposure. It describes deterioration as a
combined result of species, thickness, sunlight, slope, ventilation, overhang,
retained moisture, moss/lichen, pollutants, and maintenance. Rain, wind-carried
grit, fungi, and ultraviolet exposure preferentially erode softer wood.

Therefore checks, cup, grey colour, softened relief, damp darkening, and moss
should be correlated rather than independently hashed:

- Long checks follow the local grain and commonly begin at an exposed end or
  defect.
- Wider, poorly oriented, or lower-grade pieces may cup more; a well-selected
  split piece should remain comparatively stable.
- Weathering is primarily a roof-space exposure field modulated by material and
  age, with smaller piece-to-piece variation. A different random grey value on
  every shingle produces patchwork, not weather.
- Persistent damp zones should connect darker albedo, changed roughness,
  softened/eroded wood relief, dirt, and possible moss. Dry sun exposure should
  produce desaturation and greying without moss.
- Moss belongs preferentially in moisture-retaining laps, shaded roof zones, and
  debris catches, not as uniform green noise.

## Practitioner workflows

### Construct the roof in a slope-local frame

SideFX discussions of procedural roof tiles derive tile counts and steps from
the physical width and height of a roof primitive, then create points in a
consistent local frame. Another SideFX discussion identifies boundary pieces
from topology so offset rows can be clipped or completed at roof edges.
[[SideFX, *How to project rooftiles*](https://www.sidefx.com/forum/topic/50916/?page=1)]
[[SideFX, *Tile/roof builder*](https://www.sidefx.com/forum/post/208080/)]

These are forum practitioner reports rather than formal benchmarks, but the
technique transfers directly. Define each roof plane with:

- `V` along the steepest down-slope direction;
- `U` along the course, perpendicular to `V` in the plane;
- both coordinates measured in metres from a stable semantic origin;
- a shared course phase for connected faces where construction requires
  continuity.

Do not use world XZ as the final UV contract. It changes apparent dimensions
with slope and orientation, cannot intentionally resolve ridges/hips/valleys,
and gives rotated roofs a different grain/course relation. Curved or conical
roofs should use slope distance and arc length, not Cartesian projection.

### Build wood from directional structure, then derive dependent channels

A SideFX wood-fracture workflow finds each piece's longest edge, orients it into
a local coordinate system, and only then applies directional fracture. That is a
useful general rule: fibres, checks, and anisotropic features must be evaluated
in piece-local grain coordinates.
[[SideFX, *Procedural wood fracture*](https://www.sidefx.com/forum/post/229240/)]

Not Lonely's Substance Designer wood breakdown separates fibres, an ageing mask,
colouring, normals, and roughness while using directional gradients to maintain
wood flow.
[[Not Lonely, *Procedural Wood Material in Substance Designer*](https://www.not-lonely.com/blog/tutorials/procedural-wood-substance-designer/)]
Wes McDermott's procedural wood workflow similarly begins with a coherent
grayscale/height structure, then derives normal, ambient occlusion, roughness,
and finally colour relationships from it.
[[Pixel Fondue, *Creating a Procedural Wood Material in Substance Designer*](https://www.pixelfondue.com/blog/2017/8/24/creating-a-procedural-wood-material-in-substance-designer)]

The transferable graph is:

1. Generate course and shingle identity in metric coordinates.
2. Generate construction height: undercourse, lap, thickness, tail bevel,
   restrained cup and twist.
3. In each shingle's local frame, add a broad split-plane undulation, finer
   longitudinal fibres, rare growth-ring influence, and causally placed checks.
4. Derive normals and contact occlusion from that height at the target texel
   density.
5. Build weather/exposure fields in roof space, then modulate albedo and
   roughness with the underlying wood and construction masks.
6. Add moss or dirt from moisture/contact/catchment masks, not from independent
   noise.

Adobe's Mark Foreman material notes are consistent with this mask-driven
approach: wood grain is adjusted for scale, age, and wear, while moss is placed
using edge/gap masks in roof and floor materials.
[[Adobe Substance 3D, *Mark Foreman's Designer tips and tricks*](https://www.adobe.com/learn/substance-3d-designer/web/mark-foreman-s-substance-3d-designer-tips-and-tricks)]

### Do not let unresolved normals turn into sparkling roughness

Ready at Dawn's GDC material presentation recommends modifying roughness in
response to normal-map variation to reduce specular aliasing.
[[Matt Pettineo, GDC 2014, *Crafting a Next-Gen Material Pipeline for The Order: 1886*](https://media.gdcvault.com/GDC2014/Presentations/Pettineo_Matt_Crafting_A_Next-Gen.pdf)]
This matters for narrow fibres, checks, and hard lap lips: at distance they
cannot simply disappear while leaving the same glossy response.

At each mip level, recompute or correctly renormalize filtered normals and
increase effective roughness by the unresolved normal variance. Filter base
colour in linear light rather than averaging sRGB-encoded bytes. Test
course/contact occupancy explicitly so the silhouette rhythm does not pulse as
the camera moves.

## Geometry versus texture

The texture should own the repeating field:

- course exposure and broad overlap relief;
- shingle boundaries and recessed joints;
- split-face grain and restrained tool/fibre structure;
- minor tail variation;
- dry/grey/damp colour and roughness response;
- limited contact dirt and moss.

Geometry should own features whose silhouette, shadow, or water-shedding logic
remains visible:

- eave tails and doubled starter courses near the player;
- verges/rakes, ridge caps, hips, valleys, dormer cheeks, chimney flashing or
  abutments;
- roof poles and stones for a `Legschindel` preset;
- rare lifted, missing, slipped, repaired, or severely cupped hero pieces.

A cheap implementation need not instantiate every shingle. LOD0 can use a few
deterministic edge strips or relief rows at eaves and prominent boundaries over
the continuous roof plane. LOD1 can keep the height/normal course field with no
piece geometry. LOD2 should retain only the broad course cadence, average
species/weather colour, and stable roughness. Individual checks, nail heads, and
tiny moss islands should have vanished before they become subpixel flicker.

This division also avoids the common failure where a flat texture says “layered
shingles” but the eave, ridge, and valley remain a single impossible slab.

## Channel relationships and palette

Recommended channel logic:

- **Base colour:** species- and age-bounded wood colour; modest piece variation;
  roof-space exposure greying; damp darkening; soot or repair only where
  justified. Do not bake AO or strong lap shadows into albedo.
- **Height/normal:** dominant course overlap and contact, then split-plane
  relief, cup/twist, and fibres. Checks are narrow depressions aligned with
  grain. Grain direction is down the shingle/slope, never world-axis noise.
- **Roughness:** dry weathered wood is high but not uniform. Fresh/cut repairs
  can differ; eroded fibre and greyed surface generally broaden response.
  Wetness may darken and temporarily reduce roughness even where biological
  weathering makes the dry substrate rough. Treat those as different states.
- **AO/contact:** restricted to actual overlaps, joints, deep checks, and
  accumulated debris. It should not become a global grime multiply.
- **Metallic:** zero for the wood field. If exposed iron fasteners are ever
  represented, they need their own material semantics rather than raising
  metallic over nearby wood pixels.
- **Moss:** a material layer with colour, height, and roughness response, driven
  by shade, moisture persistence, lap catchment, and age. It should not be used
  as generic colour variety.

Keep palettes constrained by species/preset: for example, split oak, larch,
spruce/fir, fresh repair, and aged grey are meaningful variants. The current
generic “weathered softwood” description is a valid preset but should not be the
only historical identity.

## Concrete implementation recommendations

1. **Fix the sampling contract before adding detail.** Bind the generated
   texture set to `RoofMaterial::TimberShingle` and use metric slope-local UVs.
   Either sample the declared 3.2 m repeat or change the recipe and its tests
   deliberately; do not silently compress it through the generic 2.0 m roof
   mapping.
2. **Declare construction presets.** At minimum distinguish a default short,
   nailed, double/triple-lapped split-shingle roof from long weighted
   `Legschindel`. The latter requires geometry and should not be faked by
   rescaling the same atlas.
3. **Make split wood the default for the period.** Keep faces comparatively
   planar and dressed. Add broad low-amplitude split relief and longitudinal
   fibre hierarchy, with rare knots or cross-grain defects.
4. **Enforce coverage.** For every seam, test that an overlying course covers it
   and that randomized row offsets do not create long aligned leakage paths.
   Preserve the current shared boundary ownership.
5. **Correlate damage.** Derive check probability/length and cup from width,
   ring/fibre orientation proxy, grade, age, and moisture cycling. Start most
   checks at exposed tails. Keep damaged pieces uncommon in a maintained roof.
6. **Move weathering into roof space.** Use deterministic low-frequency exposure
   fields plus semantic masks for eave/ridge, sun aspect, persistent shade, wet
   laps, chimney soot, and repairs. Retain only restrained per-piece tint.
7. **Resolve boundaries in geometry.** Add a small, deterministic roof-detail
   layer for eaves, verges, ridges, hips, valleys, and openings. These should
   share the same metric course phase as the texture.
8. **Correct the mip pipeline.** Linear-light colour filtering,
   decoded/renormalized or height-derived normal mips, and normal-variance-aware
   roughness are required before fine grain is production-safe.
9. **Use purposeful LODs.** LOD0 may have edge relief and rare hero defects;
   LOD1 is texture-only but keeps lap and grain; LOD2 keeps stable broad cadence
   and colour mass. Fade or dither transitions and use anisotropic filtering on
   oblique roof planes.
10. **Keep roofscape variety outside the texture hash.** Building generation
    should choose construction/species/age presets from region, wealth,
    function, fire policy, and maintenance state. A single roof should then
    remain internally coherent.

## Acceptance tests

### Deterministic and structural

- Same seed and parameters produce byte-identical outputs; periodic edges match.
- Measured shingle width and visible course exposure in rendered world space
  agree with recipe values across roof slopes and rotations.
- Every field joint is covered by the intended number of layers; no vertical
  seam remains continuously exposed across consecutive courses.
- Piece boundaries have one owner, no cracks, and no coplanar duplicate surface.
- Grain, fibres, and checks align with each piece's local down-slope axis.
- Checks start preferentially at exposed ends/defects and remain a bounded
  minority.
- Cup/twist amplitude is bounded and correlated with the selected
  construction/grade rather than uniformly random.

### Channel and filtering

- Metallic is zero on wood; base colour contains no baked AO.
- Height, normal, roughness, dampness, and moss masks have expected causal
  correlation without becoming duplicates.
- Normal mip vectors remain unit length within tolerance after decoding.
- Linear-light reference downsamples match generated colour mips.
- Roughness does not decrease when high-frequency normal variance becomes
  unresolved.
- At a moving-camera distance sweep, course rhythm converges smoothly without
  sparkling fibres, crawling checks, disappearing joints, or moss scintillation.

### Visual review matrix

Render fixed-seed roofs under overcast, low grazing sun, and wet/darkened
conditions at:

- close eave and verge;
- oblique whole roof;
- ridge/hip/valley/dormer intersection;
- street distance at LOD0, LOD1, and LOD2;
- aerial city distance.

Review at least a new split-shingle roof, an aged grey roof, a damp shaded roof
with restrained moss, a repaired roof, and the distinct long weighted-shingle
preset. The acceptance reviewer should reject exaggerated random roughness,
cross-grain scratches, exposed nail polka dots, uniform green moss, impossible
flat boundaries, synchronized vertical seams, and visible LOD colour shifts.

## Evidence, inference, and project decisions

**Evidence:** Historical and preservation sources support split manufacture,
grain following, species and system variety, substantial overlap, concealed or
system-specific fixings, and causal moisture/weather effects. Practitioner
sources support metric local construction frames, topology-aware boundary
handling, directional material coordinates, height-first dependent channels,
mask-driven weathering, and normal-variance-aware roughness.

**Inference:** A restrained split-and-dressed short-shingle preset is a better
general default for a 1544 German setting than a uniformly rough shake field.
Roof-space exposure should dominate ageing, and wood roofs should coexist with
other coverings rather than paint the whole city uniformly. Exact species
proportions, urban prevalence, dimensions, and local fire restrictions require
location-specific historical research.

**Repository decisions:** The 3.2 m recipe scale, preset API, UV origin
convention, geometry boundary budget, weather masks, LOD thresholds, and mip
implementation are engine choices. They should be made explicit and tested; none
is dictated by the historical sources. The highest-value first step is to make
the existing structurally promising recipe appear at its declared physical scale
on a slope-local roof and to give its boundary construction a real silhouette.
