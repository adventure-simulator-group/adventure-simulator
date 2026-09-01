# Slate roof procedural texture prior art

## Scope

This report concerns exactly the `SlateRoof` procedural surface: hand-split
roofing slate arranged as an old German (`Altdeutsche`) covering for buildings
where that material and craft are plausible around 1544. It covers period
shapes and layouts, overlap and fastening logic, cleavage, chips and geological
color, weathering, the division between geometry and texture, causal PBR
channels, metric slope-local UVs, tiling, mipmaps, and LOD behavior.

It does not treat “slate” as a generic blue-gray checker or as a universal
German roof. Roofing slate is a quarry product with a regional supply geography,
and the historical evidence indicates that its expense long concentrated it on
churches, castles, palaces, towers, civic buildings, and other high-status work.

## Repository facts and constraints

The following are facts observed in this worktree, not claims from external
sources.

- `SlateRoof` produces 512 x 512 albedo, OpenGL tangent-space normal,
  normalized height, and ARM maps. The recipe declares a 4.8 m square period
  and a 12 mm represented height range.
- The pattern has 28 pieces per nominal course and 28 courses per period, so
  its tests assert approximately 171 mm visible width and 171 mm course
  exposure. It creates a course rise of two course-heights across one repeat,
  about 343 mm over 4.8 m if recipe scale is honored.
- Each piece has a stable hashed identity, mildly irregular side boundaries,
  an asymmetrically clipped heel, sparse small edge chips, low-amplitude planar
  tilt, and a three-frequency sinusoidal “cleft” field. The front course rises
  toward its lower lip and reveals a recessed under-course.
- Color is a narrow cool dark gray-blue range. Per-piece mineral and cool-shift
  hashes affect albedo, while roughness responds to mineral variation, edge
  wear, contact, and absolute cleavage. AO darkens contacts; metallic is zero.
- The current pattern repeats one nominal stone and course scale. It does not
  diminish stone/course size from eave to ridge, maintain roof-level quarry
  batches, expose grain/cleavage direction, distinguish sawn from hand-dressed
  edges, represent nail/peg holes or fasteners, or include pyrite, oxide stains,
  delamination, lichen, moss, soot, repairs, slips, or missing slates.
- Numeric tests cover deterministic periodicity, visible scale and height
  range, recessed laps/contacts, nonmetallic channel variation, and mip count.
  The ignored evidence exporter produces separated maps, interpreted renders,
  2 x 2 repeats, and 128/64 px downsamples, but not actual roof views, motion,
  roof boundaries, or LOD transitions.
- The common `image_rgba_mipped` helper byte-averages all channels. Thus sRGB
  albedo is averaged in encoded space, normals are averaged as colors rather
  than filtered/reconstructed as directions or slopes, and perceptual
  roughness ignores unresolved normal variance.
- Both detailed and LOD building meshes project roof UVs from world `X,Z / 2.0
  m`, while the recipe declares a 4.8 m period. If bound unchanged, its intended
  171 mm width/exposure would become about 71 mm and its 343 mm course rise per
  repeat about 143 mm.
- World-planar XZ mapping also violates the source comment that texture V runs
  down-slope. It cannot maintain physical distance or course orientation across
  roofs of different yaw and pitch, and it does not provide a deliberate seam
  for curved or conical roofs.
- Tactical materials currently route both `RoofMaterial::Slate` and
  `RoofMaterial::Lead` to the same dark checker. Neither `slate_roof` nor the
  separate `lead_sheet` procedural recipe is bound there, and the generated
  slate normal/ARM maps are unused.
- Roof surfaces are closed planar prisms with semantic faces, cutouts,
  enclosures, dormers, and material identities. They have no slate-specific
  geometry at eaves, verges, ridges, hips, valleys, chimneys, dormers, or
  repair/missing-piece locations.

These are system constraints. A better standalone bitmap cannot correct the
wrong size and direction, make a flat eave reveal overlapping stone edges, or
stop slate and lead from sharing a placeholder material.

## Historical and material evidence

### Slate in 1544 is regional and status-sensitive

The International Union of Geological Sciences identifies historic German
roofing-slate sources from the Rhenish Slate Mountains through the Harz,
southern Thuringia/Saxony, and northeastern Bavaria. It describes traditional
colors as dark blue, gray to black, with rarer green and red material and a
glossy splitting surface. It also records medieval slate-roofers' guilds at
Trier, Goslar, and Cologne and cites high-status medieval slate uses including
Marksburg
([IUGS, *German Roofing Slate*](https://iugs-geoheritage.org/geoheritage_stones/german-roofing-slate/)).

The German Foundation for Monument Protection likewise locates German slate
regions in the Eifel, Hunsrück, Sauerland, Taunus, Harz, Franconian and
Thuringian Forests, Ore Mountains, and Upper Lusatia. It states that the craft
was historically used chiefly for churches, palaces, castles, and other
representative buildings because slate was a luxury product; only in the
nineteenth century did it spread broadly through ordinary rural construction
([Deutsche Stiftung Denkmalschutz, *Das Licht feiert den Schiefer*](https://www.monumente-online.de/de/ausgaben/2006/4/das-licht-feiert-den-schiefer.php)).

For 1544 city generation, therefore:

- weight slate heavily near producing regions and navigable/trade supply, not
  uniformly across Germany;
- favor churches, castles, towers, town halls, wealthy institutions, and select
  elite houses outside cheap local supply;
- make quarry/lithology a roof-level material identity so neighboring roofs may
  share a recognizable source;
- allow slate on complex and steep roof geometry, where thin hand-shaped pieces
  and skilled detailing are especially useful;
- do not turn every distant roof blue-gray merely to diversify clay tile.

The RDK historical roof survey says the oldest surviving old German slate roofs
appear to include sixteenth-century examples in the Rheingau, one bearing an
inscription dated 1582
([RDK Labor, *Dach*](https://www.rdklabor.de/wiki/Dach)).
That is close evidence for the period and technique, but not proof that every
modern codified detail of `Altdeutsche Deckung` existed unchanged in 1544. The
recipe should be labeled a historically bounded reconstruction/preset, with
regional distributions kept explicit.

### Old German covering is not a uniform scale-tile grid

The Foundation's craft description says old German cover stones were cut by
hand and eye, placed in generally obliquely rising courses (`Gebinde`), and
gradually decreased in height from eave to ridge; widths could vary as well.
It contrasts this with cheaper standardized template slate introduced in 1845
([Deutsche Stiftung Denkmalschutz](https://www.monumente-online.de/de/ausgaben/2006/4/das-licht-feiert-den-schiefer.php)).
The modern craft description likewise distinguishes free-hand imbricated stones
with varying heights and widths from uniform templates, and assigns individual
treatments to valleys, hips, verges, and ridges
([Rathscheck, *Old German-style Covering*](https://www.rathscheck.de/en/anwenden-und-verlegen/deckarten/old-german-style-covering/)).

This has direct procedural implications:

- the current obliquely rising courses are appropriate in concept;
- fixed 28 x 28 scale is not enough. Course height and piece size need a
  roof-level gradient, with larger foot stones at the eave and progressively
  smaller stones toward the ridge;
- course rise is a coherent craft parameter, not per-piece jitter. Its handed
  direction and local changes must remain drainage-valid;
- variation is constrained sorting: adjacent pieces fit, joints are broken,
  headlap is maintained, and larger or wider pieces solve higher-water and
  boundary conditions;
- exact detail units at eaves, verges, hips, valleys, and ridges cannot be
  created by wrapping a uniform field texture through those boundaries.

Historic Environment Scotland's materially analogous conservation guidance
explains the engineering logic of diminishing courses: larger slates at the
bottom receive greater water flow; larger pieces are also required on shallow
slopes, and wider units are used at junctions for secure fixing. It emphasizes
that quarry source determines distinctive pattern, color, and texture
([Engine Shed, *Slate Roofs*](https://www.engineshed.scot/building-advice/building-components/roofs/slate-roofs/)).
The Scottish pattern is not itself evidence for a 1544 German layout, but its
water, overlap, and fastening constraints are transferable physical evidence.

### Overlap and fasteners form a concealed water-shedding system

Slate covering is an imbricated system. Side joints in one course must be
covered by the course above, and headlap must keep water from reaching concealed
fastener holes. Historic Environment Scotland describes traditional attachment
to wood with a single nail near the slate head and notes that proper overlap
keeps water away from the hole. It identifies poor slate-size selection,
insufficient overlap, nail corrosion, damage around holes, and failed backing
as causes of slips and leaks
([Engine Shed](https://www.engineshed.scot/building-advice/building-components/roofs/slate-roofs/)).

The exact fastening metallurgy and boarding/lathing system in a German 1544
fixture must be established per region and building; later conservation
practice should not be copied literally. What transfers visually and
procedurally is:

- holes and fasteners normally sit in the concealed head, not dotted over the
  visible face;
- an exposed nail implies damage, repair, or invalid overlap and should not be
  routine material noise;
- a slipped slate moves as a unit and may reveal its nail hole or substrate;
- insufficient headlap and aligned side joints are construction failures that
  deterministic layout tests can detect;
- a missing slate is geometry/instance state, not a black height-map cell.

### Cleavage, grain, chips, and lithology are correlated

Slate is useful because it splits into thin sheets along cleavage. German
roofing slate spans dark blue, gray, and black, with rare green or red quarry
families; it has a characteristic glossy splitting surface
([IUGS](https://iugs-geoheritage.org/geoheritage_stones/german-roofing-slate/)).
Hand splitting and dressing yield shallow planar undulation, aligned cleavage
ridges, variable thickness, and struck edges. They do not yield isotropic
“rock noise” or pillow-shaped tiles.

Quarry source must drive together:

- base hue/value and any stable mineral bands;
- cleavage direction, spacing, and sheen anisotropy proxy;
- achievable split thickness and edge character;
- inclusion type and weathering response;
- the range of piece sizes selected for the roof.

The National Slate Association distinguishes stable, semi-weathering, and
weathering colors, while warning that high concentrations of oxidizable pyrite
or calcium carbonate can cause staining or accelerated decomposition
([National Slate Association, *Slate Colors*](https://www.slateassociation.org/slate-colors/)).
This is North American trade guidance, not evidence for German quarry frequency,
but it demonstrates that brown/tan weathering and orange iron staining must be
linked to mineralogy rather than sprinkled uniformly across all slate.

The current three sine waves are an affordable cleft seed, but they should be
reoriented by a stable per-roof quarry/grain frame and broken into broad split
planes plus sparse ridges, pits, and inclusions. Chips should expose the same
lithology, change silhouette/height at the edge, and create aligned fresh
fracture normals. White painted rims or uncorrelated bright speckles are wrong.

## Practitioner workflows

### Height and ownership first

A procedural slate material by Abderrezak Bouhedda exposes shape switching,
height construction, and optional moss/lichen while retaining a fully
parameterized graph
([Bouhedda, *Slate Rooftile Material*](https://abderrezakbouhedda.artstation.com/projects/n0Nedo)).
The useful pattern is to separate semantic shape/layout from weathering, then
derive related maps from shared masks:

`roof course schedule -> piece ownership -> overlap -> piece profile/cleavage -> damage -> height`

and then:

`height + quarry + exposure + repair masks -> normal, AO, roughness, albedo`.

The current stable piece identity and explicit front/under-course selection are
good foundations. Extend each sample with roof-course fraction, quarry/batch,
head/side/lower-edge distances, fastener-zone ownership, cleavage frame,
repair-age, and exposure masks. Avoid independently seeded channel noise.

### Roof basis and boundary pieces are procedural geometry problems

SideFX roof builders orient repeated pieces to roof primitives, derive count
and step from physical piece size, control point orientation, and identify line
endpoints for special edge handling
([SideFX, *How to project rooftiles on a primitive?*](https://www.sidefx.com/forum/topic/50916/?page=1);
[SideFX, *Tile/roof builder*](https://www.sidefx.com/forum/post/208080/)).
Those are practitioner discussions, not historical sources, but they isolate
the relevant production contracts: roof-local basis, physical spacing, course
offset/rise, clipping, and explicit boundary stones.

Slate's old German layout strengthens the case for semantic geometry because
eave foot stones, left/right verge stones, junction stones, valley/hip pieces,
and diminishing course schedule differ systematically. A texture can represent
the interior field cheaply, but it cannot make its exposed boundaries true.

## Geometry versus texture division

### Texture responsibilities

Use the procedural surface for:

- ordinary interior field coverage on broad planar or smoothly parameterized
  roof regions;
- top/under-piece overlap and narrow concealed contacts;
- shallow split-plane relief, cleavage ridges, mineral inclusions, and
  restrained
  struck-edge variation;
- quarry/batch color, nonmetallic roughness, contact AO, and small face chips;
- low-frequency weathering driven by a separate roof-space coordinate field.

### Geometry responsibilities

Use LOD0 roof trim or sparse instances for:

- overlapping eave/foot-stone silhouette and exposed slate thickness;
- hand-shaped verge/`Ort` stones, ridge, hip, valley, and abutment
  treatments;
- chimney, dormer, turret, and roof-opening intersections;
- conspicuous slipped, lifted, broken, missing, or repaired pieces;
- complex conical/curved areas where the course schedule visibly converges;
- fastener or clip geometry only where damage/repair exposes it.

Do not instance every slate over the whole city by default. A cheap hybrid uses
the analytic field for broad slopes, a generated strip for eaves and verges,
semantic ridge/hip/valley pieces, and a very small number of damage instances.
LOD1 can simplify trim cadence; LOD2 retains only roof silhouette, broad quarry
color, diminishing-course value rhythm if resolvable, and stable roughness.

## Fasteners, repair, and weathering

Later conservation evidence repeatedly finds that roof failure often comes from
fasteners and support rather than uniformly eroded stone. Historic Environment
Scotland lists impact, wind lift, nail sickness, cracked nail holes, and decayed
substrate; SPAB adds delamination along poor slate layers and mortar-torching
decay
([SPAB, *True slate roofing*](https://www.spab.org.uk/advice/true-slate-roofing)).

Represent these as different phenomena:

- **Slipped slate:** whole-piece translation/rotation down-slope, usually
  sparse;
  may reveal a dark under-course or backing and a concealed hole.
- **Delamination:** thin edge flakes following cleavage, with local roughness
  and normal change; not a round crater.
- **Impact/break:** angular fracture from an edge or flaw, causally shared by
  height, normal, albedo, and roughness.
- **Fastener stain:** only where iron-bearing historic fastening is specified
  and water transports oxidation from a hole. It must follow gravity/runoff and
  should not repeat once per visible slate.
- **Repair:** replacement units match size, texture, thickness, weight, and
  quarry color where possible but can form a mild coherent patch rather than
  random salt-and-pepper variation.
- **Moss/lichen:** roof-space patches favored by shade, persistent moisture,
  sheltered overlaps, and rough split surfaces. Moss adds small height and high
  roughness; thin lichen mostly changes color/roughness.
- **Soot:** requires a chimney or combustion source and downwind/down-slope
  transport. It is not part of base slate.

The NPS preservation brief notes faster deterioration under severe sun, wind,
and rain exposure and identifies inclusions plus mechanical agents as combined
causes
([NPS, *Preservation Brief 29: Historic Slate Roofs*](https://www.nps.gov/orgs/1739/upload/preservation-brief-29-slate-roofs.pdf)).
Its exact North American fastener chronology is not transferable to 1544
Germany, but the causal exposure model is.

Avoid picturesque over-aging. A maintained 1544 civic or ecclesiastical roof
should usually be coherent, with sparse repairs and defects. Heavy lichen,
delamination, rust staining, and missing units need roof age, lithology,
exposure, and maintenance justification.

## Causal PBR construction

### Height and normal

- Allocate the 12 mm range first to overlapping plate height and visible
  thickness, then to millimeter-scale split planes, and only then to chips or
  deposits. Slate is thin and planar; it should not read as rounded ceramic or
  cobble.
- Build cleavage from elongated, piece-local bands/terraces aligned with the
  quarry/grain frame. Mix a few broad planes with restrained finer ridges rather
  than isotropic multi-octave noise.
- Derive normals from metric height. Preserve the discontinuity at actual plate
  overlaps, but let smaller cleavage frequencies filter out before aliasing.
- Geometry owns the silhouette at eaves, verges, and missing pieces.

### Albedo

- Select a geologically coherent roof palette: German slate can be dark blue,
  gray, or black and more rarely green/red, not one universal cool RGB triplet.
- Keep split-plane sheen and lighting out of albedo. Broad quarry banding and
  inclusions may change intrinsic color; contact shadows belong to lighting/AO.
- Link brown/tan weathering and oxide stains to applicable mineral/exposure
  masks. Link fresh chips to the same stone body rather than a generic white
  rim.

### Roughness, AO, and metallic

- Slate is dielectric (`metallic = 0`); occasional pyrite inclusions are too
  sparse and context-dependent to justify metallic roofing slate globally.
- Roughness responds to cleavage microfacet direction, quarry finish,
  delamination, wetness, biological film, soot, and mineral alteration. It must
  not simply invert albedo.
- Approximate cleavage's directional sheen through anisotropy if the renderer
  later supports it; otherwise encode elongated normal statistics and a
  restrained roughness range without making every plate glossy.
- AO belongs in tight laps and deep fractures. Do not bake broad eave/ridge
  shadow or directional light into albedo.

## Metric slope-local UV contract

Every roof field needs an explicit local frame:

- `V` measures metres down the steepest descent direction on the roof
  surface;
- `U` measures metres along the course in the roof plane;
- `U,V` distance follows the sloped surface, not its horizontal XZ
  projection;
- all triangles belonging to one semantic slope share origin, handedness,
  course-rise schedule, and eave-to-ridge fraction despite cutouts or
  tessellation;
- opposing slopes may use the same quarry/preset but have independent drainage
  frames;
- conical/turret roofs use arc length around the slope for U and slope length
  for V, with a deliberate low-visibility seam and convergence rule;
- boundary geometry samples the same roof field so its color and cleavage match.

Old German diminishing courses require more than a repeating UV. Supply
normalized eave-to-ridge distance as a roof-space parameter, then compute local
piece width, exposure, and course height from a monotonic schedule. The tileable
microtexture can still repeat inside each piece. Course rise and handedness are
roof-level craft parameters.

Replace the generic 2.0 m roof repeat with the recipe's physical scale or a
direct metric material contract. Add a deterministic test proving that roof yaw
and pitch do not change piece size and that V always runs down-slope.

## Tiling, mips, and LOD

### Avoiding repetition

The overlap rhythm repeats; the quarry and roof history should not. Break
obvious repetition with:

- roof-level quarry/batch parameters shared across neighboring supplied roofs;
- eave-to-ridge diminishing size and course schedule;
- stable piece IDs in semantic roof coordinates rather than per 4.8 m patch;
- sparse repairs/damage selected over the whole roof;
- building-space moss, soot, runoff, and mineral-alteration masks;
- a few compatible course/edge variants, never arbitrary rotations that reverse
  overlap or cleavage/drainage orientation.

The current oblique wrap is clever, but fixed-size hashed variation still
repeats every 4.8 m. A long-roof test must show the whole period under neutral
light and compare neighboring roofs with shared versus different quarry seeds.

### Semantic mipmaps

Generate mips by channel meaning:

- filter albedo in linear light before re-encoding sRGB;
- filter metric height first and derive/reconstruct each normal mip, or use
  slope-space normal filtering;
- preserve mean plate overlap without creating half-height phantom joints;
- filter AO as visibility;
- increase effective roughness when cleavage/overlap normals become subpixel.

Ready at Dawn's GDC material pipeline explicitly modifies roughness to suppress
specular aliasing from normal variation
([Pettineo, *Crafting a Next-Gen Material Pipeline for The Order: 1886*, GDC 2014](https://media.gdcvault.com/GDC2014/Presentations/Pettineo_Matt_Crafting_A_Next-Gen.pdf)).
This is particularly important for slate: repeated bright split-plane glints and
diagonal course edges will crawl at oblique angles if normal bytes are simply
averaged. Anisotropic filtering helps the pitched roof footprint but cannot
repair invalid mips.

### LOD behavior

- **LOD0:** full metric field, eave/verge/ridge/hip/valley geometry, quarry-
  coherent cleavage, sparse repair/damage, and roof-space weathering.
- **LOD1:** simplified boundary strips and junctions; retain course rise,
  diminishing scale, plate overlap normal, and broad quarry/weathering; discard
  fine cleavage and tiny chips.
- **LOD2:** stable dark roof mass and silhouette with only low-frequency quarry
  color and subdued course-direction roughness/normal energy. Do not resolve
  individual 70–170 mm scales if they become a flickering checker.
- Cross-LOD transitions preserve average linear albedo, effective roughness,
  roof outline, course handedness, and eave-to-ridge value distribution.

## Recommended implementation sequence

### 1. Define roof-level historical presets

Create an old German slate preset with quarry region, lithology palette,
cleavage frame, course-rise handedness, eave/ridge size range, head/side lap,
thickness range, and boundary style. Keep later template-scale coverings out of
the 1544 preset unless separately evidenced.

### 2. Repair the coordinate and runtime binding

Generate metric slope-local UVs plus eave-to-ridge fraction for detail and every
LOD. Bind `slate_roof` to `RoofMaterial::Slate` and keep `Lead` on its own recipe.
Validate constant physical scale across yaw/pitch and correct down-slope V.

### 3. Implement diminishing causal layout

Retain deterministic piece ownership, but schedule larger foot stones and
smaller ridge stones, constrained widths, broken joints, valid headlap, and
explicit course rise. Add roof-level quarry/batch and piece-local cleavage.

### 4. Add boundary geometry and restrained defects

Generate cheap eave/verge strips and distinct ridge/hip/valley/junction
treatments from semantic roof data. Add only sparse whole-piece slips, missing
units, angular breaks, and repair patches where state warrants them.

### 5. Add exposure and semantic filtering

Drive lichen/moss, soot, water staining, and mineral alteration from roof-space
inputs. Replace byte-average mips with linear-color, height/normal-aware,
visibility-aware, and variance-aware filters, then prove motion stability.

## Acceptance and regression tests

### Deterministic numeric tests

- Output is deterministic, but roof-level variation does not restart every 4.8
  m.
- Piece height/width decrease monotonically within preset bounds from eave to
  ridge; larger foot stones, wider junction stones, and explicit boundary units
  are selected where required.
- Side joints are covered by the correct over-course, headlap remains above its
  minimum, and fastener zones remain concealed for intact pieces.
- Course rise and handedness remain coherent; layout rejects drainage-invalid
  rotations, aligned leakage paths, holes, and overlap inversions.
- One metre measured on roofs of different yaw/pitch maps to one metre of
  texture. V dot roof-descent is positive. Cutouts do not reset the field.
- Slate and lead resolve to different procedural material sets.
- Metallic is zero. AO minima coincide with actual contacts. Cleavage, chip,
  delamination, oxide, and repair masks correlate across the appropriate maps.
- Weathering varies in roof space rather than at the base texture period.
- Linear-albedo, reconstructed-normal, visibility, and variance-aware roughness
  mips satisfy bounded mean/energy tests.
- LODs preserve average albedo, effective roughness, silhouette, course
  handedness, and eave-to-ridge size/value trends.

### Visual fixtures

Capture under fixed neutral, grazing, wet-look, and overcast lighting:

- a true-scale old German reference slope with ruler, side profile, and visible
  course rise/diminishing stones from eave to ridge;
- dark-blue, gray-black, and a separately evidenced rarer quarry palette, each
  with coherent cleavage rather than random recoloring;
- a 2 x 2 repeat and at least 20 m roof run for seams and repeated piece damage;
- the same slope at four yaws and several pitches, proving slope-local scale;
- eave, both verges, ridge, hip, valley, turret, dormer, chimney, and abutment
  closeups;
- maintained, repaired, slipped, delaminating, lichen-damp, mineral-stained,
  and chimney-sooted variants with legible causes;
- LOD0/LOD1/LOD2 at matched apparent size plus slow approach/retreat and shallow
  grazing camera motion;
- a city panorama demonstrating scarcity/status/region rules rather than a
  universal slate blanket.

An independent visual reviewer should reject a uniform fish-scale stencil,
fixed scale from eave to ridge, machine-perfect nineteenth-century template
patterns on a 1544 preset, side/uphill overlap, pitch-dependent scale, random
rock noise, pillow-shaped plates, white chip outlines, exposed nails on every
unit, periodic rust/moss/soot, texture crossing ridges and valleys, flat printed
eaves, slate and lead sharing one material, sRGB-darkened mips, sparkling cleft
normals, moire, or LOD flashes.

## Evidence, inference, and project decisions

- **Evidence:** roofing slate has a defined German quarry geography and medieval
  craft history; it was historically expensive and concentrated on high-status
  work. Sixteenth-century old German examples survive. The craft uses free-hand
  imbricated stones, obliquely rising courses, decreasing course/stone height
  from eave to ridge, variable widths, and specialized boundary details. Quarry
  source governs color, split surface, and weathering behavior. Overlap and
  concealed fastening are essential to water shedding.
- **Practitioner evidence:** procedural roofing workflows establish roof basis,
  physical piece spacing, piece ownership, height/profile, boundary units, and
  shared masks before secondary channels. GDC rendering practice treats
  unresolved normal variation as a roughness-filtering problem.
- **Inference:** the affordable credible implementation is a slope-local
  diminishing interior field plus sparse boundary geometry, quarry-coherent
  material parameters, building-space weathering, and semantic mips/LODs. Most
  failure states should be whole-piece geometry or instances, not texture holes.
- **Repository decisions still required:** 1544 quarry/supply maps and status
  weights; exact eave/ridge piece-size and lap ranges; historically appropriate
  fastener/substrate presets by region; course-rise handedness distribution;
  edge-strip triangle budget; curved-roof convergence; and acceptable roof age
  and repair distributions.

The minimum credible milestone is a real-scale, quarry-coherent old German
slate slope whose courses rise correctly and diminish from eave to ridge, whose
V axis runs down every roof regardless of yaw/pitch, whose exposed boundaries
are construction rather than printed lines, and whose cleavage, color,
roughness, weathering, and silhouette remain stable through motion and the full
LOD chain.
