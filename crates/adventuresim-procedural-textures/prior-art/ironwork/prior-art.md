# Historic forged-iron prior art

## Scope and historical contract

This report concerns exactly the `Ironwork` procedural surface. Its consumers
are forged bars, strap hinges, pintles, latches, handles, lock plates, nails,
rivets, collars, grilles, and related architectural fittings. It is not a
complete object generator, a modern rolled-steel material, cast iron, weapon or
armour steel, or a generic layer of orange rust.

For a 1544 German setting, the safe visual target is hand-worked iron whose
material identity survives beneath whatever finish, oxide, dirt, polish, and
corrosion a particular fixture has acquired. Museum objects establish that
iron door handles and plates, mortise locks, and substantial hinges are directly
appropriate forms: the Metropolitan Museum records a German iron mortise lock
with key from the early sixteenth century, German iron door handles from the
fifteenth-sixteenth and sixteenth centuries, and German folding hinges with
door bands from about 1580-1620
([Met: mortise lock with key](https://www.metmuseum.org/art/collection/search/191476),
[Met: German door handle](https://www.metmuseum.org/art/collection/search/468785),
[Met: door handle and plate](https://www.metmuseum.org/art/collection/search/468793),
[Met: folding hinges with door bands](https://www.metmuseum.org/art/collection/search/186889)).

**Evidence boundary.** These object records prove that the forms and material
belong to the period family; they do not prove that every urban doorway or
window used the same design, finish, amount of decoration, or wear. British
architectural-conservation sources below are useful evidence for wrought-iron
fabrication and deterioration, but are not frequency surveys of 1544 German
cities.

**Inference for the recipe.** The default should be quiet enough to serve both
plain domestic fittings and more elaborate work. Object-specific construction,
ornament, condition, and contact must be layered by the consuming asset. A
single aggressively pitted, hammered, or orange-rusted tile would falsely make
every hinge, grille, and latch the same object.

## Repository facts and constraints

The following are current repository observations, not external claims.

- `ironwork.rs` owns a deterministic 512 by 512 repeatable surface. One repeat
  represents 0.64 metres square, about 1.25 mm per base texel, with a declared
  full height range of 1.8 mm.
- It emits sRGB albedo, OpenGL tangent-space normal, scalar height, and packed
  AO/roughness/metallic, with ten mip levels and repeat sampling.
- Its U axis is documented as the long forging direction. The implementation
  combines broad and fine periodic noise, 38 overlapping elongated hammer
  facets, a seven-cycle sinusoidal draw signal, and two oxide-scale noises.
- Hammer facets are roughly 33-77 mm long and 12-29 mm half-width at the stated
  tile scale. Most align approximately with U; every fifth is approximately
  transverse. These dimensions describe the current generator, not a validated
  historical measurement.
- Albedo is a narrow, very dark warm-grey range. Roughness is moderately high,
  AO is nearly white except in scale recesses, and the metallic channel is 255
  everywhere, including pixels labelled as oxide.
- A generic 3 by 2 noise field chooses “contact” zones. It is not informed by a
  handle, latch, hinge, fastener, wall, hand height, or direction of movement.
- Tests establish determinism, bounded tile-edge value jumps, declared scale
  and height bounds, broad roughness variation, high AO, all-metal metallicity,
  and complete mip chains. They do not prove historically plausible forging,
  fixture-aware wear, corrosion causality, semantic mip filtering, or runtime
  appearance.
- The shared RGBA mip helper averages encoded bytes for every channel. It does
  not filter sRGB albedo in linear light, decode and renormalize normal vectors,
  compensate roughness for unresolved normal variance, or preserve categorical
  metal/oxide coverage explicitly.
- The module documentation correctly assigns silhouette, shape masks, and
  fastener geometry to the consuming mesh. The texture is the material visible
  across those forms; it cannot know where a bar ends, turns, is punched,
  welded, riveted, touched, sheltered, or embedded in masonry.
- Existing visual-review provenance records useful rejected failure modes:
  cloudy/wood-like fields, repeating diagonal stamps, cracked-leather cells,
  and conspicuous Voronoi masks. The accepted candidate is intentionally quieter,
  but its physical and semantic contracts remain to be validated.

## Material structure and forge scale

### Wrought iron is directional, but it is not wood grain

**Evidence.** Traditional wrought iron contains elongated slag inclusions and
develops a fibrous or laminated structure through repeated working. RICS notes
that the grain gives wrought iron a rough surface that mechanically retains
oxide, while Historic England conservation material describes laminations,
uneven forged sections, swellings around punched work, and corrosion that can
enter and lift those laminations
([RICS: Why wrought iron needs care despite durability](https://ww3.rics.org/uk/en/journals/built-environment-journal/wrought-iron-conservation-maintenance-repair.html),
[Historic England: Conservation of Architectural Metalwork, part 2](https://historicengland.org.uk/education/training-skills/training/webinars/recordings/webinar-on-conservation-of-architectural-metalwork-part-2-iron-gates-and-railings/)).

**Inference for the recipe.** Keep an axis-aware, very low-amplitude material
flow along the worked member, but avoid literal timber fibres or evenly spaced
machine striations. Directionality should appear mostly through stretched,
interrupted changes in reflectance and shallow relief. Strong delamination,
split ends, or lifted “roke” belongs to a damaged variant and should follow a
member's actual long axis and exposed end, not tile isotropically.

The current seven-cycle sine is too regular to stand in for working flow. A
better field would use irregular, band-limited streak segments whose wavelength,
amplitude, and phase vary slowly along U, with no guaranteed stripe crossing the
whole repeat.

### Forge oxide should be thin, dark, adherent, and stateful

**Evidence.** Getty's architectural-conservation lectures distinguish porous
red oxide formed at high oxygen concentration from compact black magnetite
formed under lower-oxygen forge conditions, and identify black oxide as a
traditional protective finish for decorative forged iron
([Getty Conservation Institute: Lectures on Materials Science for Architectural
Conservation](https://www.getty.edu/conservation/publications_resources/pdf_publications/pdf/torraca.pdf)).
Historic England's archaeological report describes hammerscale as flake and
spheroidal smithing debris recovered around forging sites
([Historic England: Hammerscale](https://historicengland.org.uk/research/results/reports/47-1992)).
Research on historic wrought iron also finds mill/forge scale surviving within
corrosion layers and investigates its reported protective effect
([Cardiff University: Mill scale on historic wrought iron](https://orca.cardiff.ac.uk/id/eprint/139761/)).

**Inference for the recipe.** The intact default may carry subdued blue-black
or brown-black oxide, with small changes in roughness and reflection, but it
should not look like a field of detached archaeological flakes. “Hammerscale”
on a smithy floor and an adherent oxide skin on a finished fitting are related
process evidence, not the same visual feature.

Use separate named masks for:

1. intact dark forge oxide;
2. thin or brushed oxide exposing iron on facet crowns and tool paths;
3. later atmospheric rust where moisture reaches iron;
4. dirt or coating over either substrate; and
5. detached or lifting scale only in an explicit neglected/damaged state.

Do not produce oxide with a thresholded cellular crack network. Patches should
have eroded, irregular boundaries and a believable hierarchy from broad film
continuity to small local loss.

## Hammer, file, punch, and drift marks

### Skilled forging leaves facets, not a universal stamped-dimple pattern

**Evidence.** The Canadian Conservation Institute notes that hammering and
finishing can leave diagnostic tool marks, while corrosion patterns can reveal
worked or stressed areas
([CCI: Identifying archaeological metal](https://www.canada.ca/en/conservation-institute/services/conservation-preservation-publications/canadian-conservation-institute-notes/identifying-archaeological-metal.html)).
Building Conservation warns that aggressive cleaning can erase original forge
scale, file marks, and polish—evidence that all three may coexist on a historic
surface
([Building Conservation: Wrought Ironwork](https://www.buildingconservation.com/articles/wroughtiron/wrought2000.htm)).

**Inference for the recipe.** Broad hammer facets should overlap coherently and
remain shallow after finishing. They can have a slight tilt and a smoother or
partly scale-cleared crown, but should not each create an isolated crater with a
dark outline. Sparse file striations may occur on fitted faces or decorated
plates, yet they need a consumer-provided “fitted/finished zone” rather than
covering every bar.

Modern “brute de forge” reference often celebrates conspicuous hammer dents as
an artistic finish. Treat it as evidence of what a forge can produce, not proof
that utilitarian sixteenth-century German hardware was routinely left with
equally exaggerated marks. The benchmark is whether the form reads as forged
iron at ordinary fixture distance before the dents read as a pattern.

### Punching and drifting alter the section

**Evidence.** Historic England notes that old punched work may show swelling
around the hole and can split along laminations when badly worked. That effect
changes the member's cross-section and edge profile, rather than merely coloring
a circular spot.

**Inference for consumers.** Model or bake a punched/drifted hole with:

- an actual opening and inner wall;
- a slightly upset or uneven rim where appropriate;
- local directional distortion of the material flow;
- restrained polished contact if a pin moves in it; and
- corrosion concentrated in the sheltered annulus only when exposure supports
  it.

The base tile must contain no generic punched circles. At normal gameplay scale,
holes, nail pads, split ends, and bar tapers are silhouette details.

## Edges, joins, welds, collars, and fasteners

### Construction features belong to topology or object-space masks

**Evidence.** Building Conservation describes rivets as headed pins through
joined pieces and collars as strips wrapped around parallel members. It also
distinguishes traditional construction from modern replacement practices and
notes that forge welding was only one of several joining solutions
([Building Conservation: Wrought Ironwork](https://www.buildingconservation.com/articles/wroughtiron/wrought2000.htm)).
SideFX hard-surface discussions reach the same digital ownership conclusion:
bevels require selected topology, and procedural groups must be derived from
stable geometric properties if topology can vary
([SideFX forum: procedural bevel](https://www.sidefx.com/forum/topic/16052/),
[SideFX forum: when to model procedurally](https://www.sidefx.com/forum/topic/49285/)).

**Inference for consumers.** Give forged members real bevels, tapers, knuckles,
rolled eyes, and terminations. Generate stable semantic groups such as
`edge`, `end`, `joint`, `collar`, `rivet_head`, `punch_rim`, `moving_contact`,
and `masonry_socket`; use those groups both for geometry and texture masks.

- **Rivets and nails:** geometry at close LOD, with a head, shank/penetration
  implication, and local seating deformation. Do not scatter decorative dots.
- **Collars:** wrapped geometry with overlap or scarf logic and shadow-bearing
  thickness, not a dark band painted around a bar.
- **Forge welds:** usually a subtle scarf/flow disturbance and section change,
  not a modern arc-weld bead. Reserve a visible seam for a deliberately crude
  or failed join.
- **Hinge knuckles and pintles:** cylindrical geometry with an axis, bearing
  clearance, grease/dirt retention, and circumferential contact polish.
- **Edges:** irregularity should be low-frequency and forge-shaped. Randomly
  chipped razor edges imply brittle cast damage or severe decay.

An object generator should decide construction first, then allow the material
to react to it. A tile cannot synthesize believable hardware by itself.

## Doors, windows, and fixtures

**Evidence.** The Met records German iron mortise locks, handles with plates,
and later-sixteenth-century folding hinges/door bands. A European iron strap
hinge dated fifteenth-sixteenth century confirms the broader strap-hinge form
([Met: strap hinge](https://www.metmuseum.org/art/collection/search/475521)).
The records show significant variation in dimensions and assembly, rather than
one universal hardware module.

**Inference for the asset system.** Build a small historically bounded fixture
grammar rather than stamping one materialized prop everywhere:

- doors: strap or band hinge, pintle, latch/handle, lock plate, nails/rivets;
- shutters: lighter straps, pintles, stays, catches, and interior fasteners;
- opening casements: narrow hinges, latch plates, handles, and closure points;
- fixed or guarded openings: forged bars with collars or masonry sockets;
- gates/chests/service doors: heavier sections and more visible reinforcement.

Each fixture should expose local coordinates and masks for touch, sliding,
rotation, shelter, upward-facing water collection, wall contact, and drainage.
Wear on a frequently used handle is not the same as wear on a high hinge strap.
Bars embedded in damp masonry need a different corrosion field from an indoor
lock plate. A door's material can use the same `Ironwork` substrate while its
construction and condition remain distinct.

The research does not justify assigning elaborate iron to every humble opening.
Frequency and social distribution should be controlled by building archetype,
wealth, security need, and region—not by the texture recipe.

## Corrosion, coatings, and handling

### Rust follows water, oxygen, defects, and retention

**Evidence.** CCI states that bare iron oxidizes slowly in clean dry air, faster
in humidity, and faster again under a water film; uneven corrosion can admit
water and oxygen to the underlying metal
([CCI: Care and Cleaning of Iron](https://www.canada.ca/en/conservation-institute/services/conservation-preservation-publications/canadian-conservation-institute-notes/care-iron.html)).
Scotland's Engine Shed identifies water traps, unsealed joints and holes, dirt,
damaged paint, previous corrosion, and vegetation as recurrent causes of local
corrosion
([Engine Shed: Iron Railings and Gates](https://www.engineshed.scot/building-advice/building-components/iron-railings-and-gates/)).
Historic England reports particularly severe loss where iron was close to wall
tops or alignments and difficult to repaint, and deep local pitting beneath
overpainted corrosion
([Historic England Conservation Bulletin 06](https://historicengland.org.uk/images-books/publications/conservation-bulletin-06/conservationbulletin06/)).

**Inference for consumers.** Generate corrosion from causality fields:

1. wetness and time-to-dry from surface orientation and shelter;
2. concavity/water-trap and joint proximity;
3. masonry or timber contact, especially sockets and inaccessible backs;
4. coating or scale loss;
5. dirt retention;
6. handling or mechanical abrasion; and
7. age/maintenance state.

Horizontal shoulders, lower edges, collars, punched holes, hinge seams, and wall
sockets may accumulate water. Vertical runoff should connect those sources to
downward rust streaks and may stain adjacent plaster, timber, or stone. Do not
sprinkle equal orange specks over every orientation. Do not put vertical drips
on the underside of a horizontal latch or across members whose UV axes differ.

### Finish variants must remain explicit

**Evidence.** Conservation guidance treats paint, oil, lacquer, and wax as
distinct coatings. Historic England notes that paints were common protection
for ferrous window frames, although surviving early color chronology is hard to
establish. It also warns that modern black repainting can overwrite earlier
schemes; one conservation example reinstated an identified warm mid-grey rather
than assumed black
([Historic England: Glass and Glazing](https://historicengland.org.uk/images-books/publications/glass-glazing-conservation/glass-marketing-spreads/),
[Historic England Conservation Bulletin 06](https://historicengland.org.uk/images-books/publications/conservation-bulletin-06/conservationbulletin06/)).

**Inference for this project.** Do not encode “all iron is black” as the only
state. Provide separately parameterized or layered variants:

- intact black forge oxide with restrained wax/oil response;
- burnished or locally polished bare iron for protected interiors/contact;
- historically selected painted iron, with paint treated as a dielectric coat;
- chipped/failed coating exposing metal and then rust at causal defects; and
- neglected exterior iron with localized stable and active corrosion.

Exact paint colors and prevalence in 1544 Germany require object/building-level
historical decisions beyond this material report. A dark oxide default is safer
than inventing a universal glossy black paint, but it should not erase plausible
finished variants.

### Contact wear requires fixture semantics

**Evidence.** Historic England notes that perspiration from hands can contribute
to corrosion and discusses burnished or polished internal door furniture as a
distinct conservation condition
([Historic England: Conservation of Architectural Metalwork, part 1](https://historicengland.org.uk/education/training-skills/training/webinars/recordings/webinar-on-conservation-of-architectural-metalwork-part-1/)).

**Inference for the recipe.** Replace the current generic low-frequency
`contact_zone` with a consumer mask. Handles, latch tongues, keyway rims,
hinge bearings, sliding bolts, and frequently grasped bar regions should each
have different directional wear. Contact can remove dirt and weak oxide, smooth
microfacets, expose brighter metal, or—under sweat and neglect—accelerate local
corrosion. It should not simply brighten every hammer crown inside arbitrary
rectangles.

## Procedural-material practice

### Separate the reusable substrate from unique object history

**Evidence.** Daniel Thiger's GDC presentation advocates reusable procedural
material methods and efficient variation in Substance Designer
([GDC Vault: Creating Photorealistic Procedural Materials with Substance
Designer](https://www.gdcvault.com/play/1024844/Creating-Photorealistic-Procedural-Materials-with)).
Practitioner environment breakdowns commonly create reusable base metal in
Designer, then use unique bakes and masks in Painter for individual objects,
edge wear, dirt, and history
([80 Level: Smithy, Creating a Stylized Diorama](https://80.lv/articles/smithy-creating-a-stylized-diorama-in-substance-ue4),
[80 Level: Combining Workflows to Create a Weathered Rusty Building](https://80.lv/articles/combining-workflows-to-create-weathered-rusty-building),
[80 Level: Smart Environment Production Techniques](https://80.lv/articles/smart-environment-production-techniques)).

SideFX forum answers on variable procedural assets emphasize deriving selections
from topology rather than brittle face numbers, and using curve/path coordinates
to make a stable UV direction
([SideFX forum: when to model procedurally](https://www.sidefx.com/forum/topic/49285/),
[SideFX forum: procedural asset UV generation](https://www.sidefx.com/forum/topic/37820/),
[SideFX forum: wrapping a procedural box](https://www.sidefx.com/forum/topic/91476/)).

**Inference for this project.** Preserve the reusable `Ironwork` substrate, but
make the asset generator provide semantic construction and exposure masks. The
combined graph should proceed from causes, not independent noises:

1. member coordinate and fabrication direction;
2. quiet forged height/normal substrate;
3. intact forge-oxide coverage;
4. topology masks for edges, joins, fasteners, holes, and bearings;
5. use masks for touch, sliding, and rotation;
6. exposure masks for wetting, runoff, shelter, and material contact;
7. coating/maintenance state; then
8. correlated albedo, normal, roughness, metallic, AO, and optional damage.

Every visible mark should answer either “how was this made?”, “how was this
joined?”, “how is it used?”, or “how did water and maintenance reach it?” If it
answers none of those, it is probably decorative noise.

## PBR channel relationships

### Albedo, metallic, roughness, normal, and height have different jobs

**Evidence.** Adobe's PBR guide states that raw metal is metallic, while rust,
paint, dirt, and other matter covering it are dielectric; those covered areas
should therefore be black in a metallic map. It also distinguishes color maps,
which are gamma encoded, from data maps such as metallic and roughness, which
are linear
([Adobe Substance 3D: PBR Guide, part 1](https://www.adobe.com/learn/substance-3d-designer/web/the-pbr-guide-part-1?learnIn=1)).
Adobe defines roughness as microsurface variation rather than generic color or
height
([Adobe Substance 3D Designer glossary](https://experienceleague.adobe.com/en/docs/substance-3d-designer/using/glossary)).
Production PBR guidance likewise treats base color as free of baked lighting,
uses mainly binary metalness, and gives roughness primary responsibility for
microsurface response
([GDC 2016: An End-to-End Approach to Physically Based Rendering](https://media.gdcvault.com/gdc2016/Presentations/Bugden_Sam_AnEndTo.pdf)).

**Inference for the recipe.** The current all-255 metallic contract is wrong as
soon as `oxide` means a visible rust, paint, dirt, or thick oxide layer. Use
coverage-consistent material states:

- raw/burnished iron: metallic 1, dark iron base reflectance, roughness driven
  by finishing and use;
- intact thin black oxide: model explicitly as a surface film; with the current
  single-layer metallic/roughness shader, a conservative approximation is
  mostly dielectric coverage rather than pretending it is bright bare metal;
- orange/brown atmospheric rust: metallic 0, high and varied roughness, diffuse
  iron-oxide color, relief only where actual corrosion builds or pits;
- paint/wax/dirt: dielectric surface coverage with its own roughness and color;
  chipped holes reveal metallic iron before causal rust develops.

Do not copy height directly into roughness. A hammer facet can be shallow but
smooth, a corrosion bloom can be rough without a deep pit, and a dark oxide film
can be thin while materially changing reflection. Likewise, do not bake edge
darkening or ambient shadows into albedo. AO should remain near white on an open
bar and only darken where real micro-cavities or object-space joins justify it.

Named masks should correlate maps without making them identical. For example,
active rust may raise roughness, reduce metallic coverage, warm albedo, and add
small porous relief; a touched crown may lower roughness and expose metallic
substrate without changing macroscopic height.

## Physical scale, UVs, tiling, and mips

### Declare scale by phenomenon, not only by texture repeat

**Evidence.** Practitioner workflows use explicit texel density and verify
materials in the target engine; one environment breakdown reports 1024 pixels
per metre as a project choice, demonstrating that scale is managed rather than
guessed, not prescribing a universal value
([80 Level: Smart Environment Production Techniques](https://80.lv/articles/smart-environment-production-techniques)).

**Inference for this recipe.** Retain explicit tile metres and additionally
record plausible bands for each generated phenomenon: section drift, hammer
facet, file/tool mark, oxide island, pit, and grain. The current 0.64 m repeat is
large relative to a handle or narrow hinge. Consumers should sample member-local
coordinates at fixed world scale and vary phase/seed per forged piece so two
neighboring straps do not display the same facet at the same distance from an
end.

The declared 1.8 mm full height span may be defensible for a rough or damaged
surface but is not automatically correct for a quiet finished substrate. Visual
and quantitative review should separate sub-millimetre surface response from
millimetre-scale section deformation. Large dents, upset rims, scarf welds,
rolled eyes, and corrosion loss belong to geometry or detail bakes.

### Member-local UVs should follow forging and construction

**Evidence.** SideFX procedural-UV discussions use curve/path coordinates to
maintain a stable longitudinal UV and warn that arbitrary scaling can stretch
tiles or misalign baked bevel detail
([SideFX forum: wrapping a procedural box](https://www.sidefx.com/forum/topic/91476/),
[SideFX forum: procedural UV mapping](https://www.sidefx.com/forum/topic/52342/)).

**Inference for consumers.** Use U along the forged member and V around or
across its section. Keep texel density constant as bars change length. Split
or explicitly map end faces, rolled eyes, knuckles, plates, and abrupt section
changes. Use geometry bevels so the substrate can remain simple across faces;
do not bake a universal edge into the tile and then stretch it with the object.

For a swept bar, derive U from arc length and V from a transported cross-section
frame. For a plate or strap, use member-local planar/strip mapping. A triplanar
fallback may hide seams on incidental shapes, but it loses the meaningful
forging direction and should not be the primary contract.

### Mips must preserve meaning, not merely byte averages

**Evidence.** PBR sources distinguish gamma-encoded visible color from linear
data. Normal vectors and material categories are not scalar colors, so encoded
byte averaging is not a physically faithful downsampling rule.

**Inference for this repository.** Build semantic mip chains:

- decode sRGB albedo to linear light, filter, then encode;
- decode tangent normals, average vectors, and renormalize;
- filter height as a scalar, with an explicit choice of mean versus conservative
  extrema for displacement consumers;
- filter roughness while accounting for unresolved normal variance so distant
  iron does not acquire unstable sharp glints;
- preserve metallic/dielectric coverage for raw iron versus oxide/paint/rust;
- filter AO as visibility, not as a decorative darkening channel.

At distance, hammer facets and pits should converge to stable reflection width,
not shimmer, crawl, or turn into alternating bright/dark pixels. The final mip
should represent the average material state of the whole tile, including oxide
coverage, rather than an accidental half-metal grey with unexplained albedo.

## Recommended procedural representation

The minimum useful implementation split is:

1. **Quiet substrate:** broad section drift, intermittent U-aligned working
   flow, fine non-cellular microvariation, and sparse overlapping shallow
   facets.
2. **Forge-film state:** intact black oxide, brushed/thinned crowns, and optional
   protected bare/burnished iron. This can be seeded per member but must remain
   repeat-safe.
3. **Construction masks from the mesh:** edge/end, bend, punch, rivet, collar,
   scarf, knuckle, bearing, and masonry socket.
4. **Use masks from the fixture:** grasp, key, slide, rotation, impact, and
   inaccessible back face.
5. **Exposure masks from the scene:** up-facing wetness, shelter, concavity,
   runoff, wall/wood contact, and maintenance access.
6. **Condition layer:** intact, maintained, worn, painted, chipped, stable
   weathered, or actively corroding. The state controls material coverage rather
   than merely tinting one universal texture.
7. **Semantic outputs and mips:** all channels derived from the same named
   physical masks, then downsampled according to channel meaning.

The base texture should contain no baked rivets, weld beads, edge wear, drips,
or universal contact patches. Those are valuable only when placed by the object
that explains them.

## Deterministic tests and visual acceptance

### Automated tests

Retain repeatability and mip-completeness tests, then add contracts that detect
specific material failures:

- exact metadata for tile metres, texel size, height range, tangent convention,
  channel packing, and U-axis meaning;
- value and derivative continuity across both repeat seams for height, normal,
  albedo, roughness, and oxide coverage;
- statistical anisotropy showing restrained U-aligned structure without a
  single seven-cycle spectral spike;
- facet size/occupancy/overlap bounds in metres, with no evenly repeated stamp
  lattice and no closed cellular crack network;
- named-mask correlation tests: rust implies dielectric coverage and elevated
  roughness; polish implies lower roughness and exposed substrate only where a
  consumer contact mask exists;
- absence of independent baked-light gradients in albedo and near-white AO on
  an unoccluded flat coupon;
- normal length error bounds at every mip after decode;
- linear-light albedo mip reference checks and deterministic roughness-variance
  checks;
- metal/dielectric coverage checks through mips for controlled test patterns;
- per-member phase/seed repeatability and decorrelation between neighboring
  deterministic member IDs; and
- fixture-mask unit scenes proving that a handle, hinge bearing, rivet joint,
  and masonry socket receive different wear/corrosion while sharing the same
  substrate.

Tests should reject the documented cloudy wood, diagonal stamp, cracked-leather,
and Voronoi-cell failure modes directly—for example through spectral peaks,
closed-edge density, and isotropy metrics—rather than relying only on broad
channel ranges.

### Visual acceptance set

Review in the procedural texture lab and in representative runtime fixtures:

1. a flat material coupon under broad grazing light and a moving point light;
2. a 2 by 2 repeat at true scale, with seam overlays and false-color height;
3. a narrow strap hinge with real bevel, rolled eye/pintle, nail or rivet, and
   member-local UVs;
4. a German-period door handle/plate and latch viewed at hand distance;
5. a window casement hinge and a fixed bar/grille with collars or sockets;
6. maintained indoor, maintained exterior, and neglected exterior condition
   variants using identical geometry;
7. wet/joint/masonry-contact masks visualized separately from final shading;
8. camera distances that exercise every mip and the building LOD transition;
9. overcast, warm interior, hard sun, and moving specular environments; and
10. neighboring deterministic instances checked for phase repetition.

Acceptance questions:

- Does it read first as forged iron, not dark wood, leather, stone, or painted
  plastic?
- Are hammer facets subordinate to form and plausible at stated scale?
- Do bevels, punches, collars, rivets, and knuckles exist as construction rather
  than printed decoration?
- Does corrosion begin at believable wet, sheltered, damaged, or inaccessible
  locations and run downward across adjacent materials?
- Does touched metal polish only where a hand or mechanism can reach it?
- Do intact oxide, bare metal, paint, dirt, and rust produce coherent metallic
  and roughness responses?
- Do reflections remain stable as the camera and door/window move?
- Can the same substrate support plain and ornate fixtures without making them
  look cloned?

An independent reviewer should receive shuffled candidate captures plus the
scale/condition manifest. Approval requires correct material identity,
construction ownership, historical restraint, and temporal stability—not merely
that the texture contains visible detail.

## Common failure modes to reject

- a cloudy dark swatch whose only identity is metallic glint;
- wood-like directional grain or periodic machine brushing;
- regular diagonal hammer stamps or uniformly distributed round dents;
- cracked-leather/Voronoi borders posing as forge scale;
- orange rust sprinkled independently of water and coating loss;
- rust, paint, or dirt left metallic because the substrate is iron;
- mirrored or rectangular “contact zones” unrelated to the fixture;
- modern arc-weld beads on forge-welded or mechanically joined work;
- rivets, collars, punched holes, folds, and bevels painted into a seamless tile;
- perfectly uniform factory bar sections on close hero hardware;
- black paint assumed universal across every building and maintenance state;
- exaggerated pitting that consumes the silhouette of maintained iron;
- stretched UVs that rotate forging flow from one face to the next;
- sRGB-byte and encoded-normal averaging that produces dark mips or unstable
  glints; and
- the same 0.64 m repeat visibly synchronized across adjacent hinges and bars.

## Bottom line

The strongest prior-art pattern is not “add more metal noise.” It is to separate
a restrained, longitudinal forged-iron substrate from the topology and history
of each fixture. Black forge oxide, overlapping shallow facets, and faint worked
flow can live in the reusable recipe. Edges, holes, tapers, knuckles, collars,
rivets, scarf welds, handles, latches, and sockets belong to geometry or
object-space masks. Polish follows hands and moving contacts; corrosion follows
moisture, joints, damaged films, and maintenance access. Those same causal masks
must drive albedo, roughness, metallic coverage, normal, and height coherently,
and their mip chains must remain physically meaningful and temporally stable.
