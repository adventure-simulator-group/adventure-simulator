# Procedural building prototype

This crate is a standalone experiment. It does not participate in either the
strategic simulation or tactical runtime. It converts a high-level building
program into deterministic semantic data, then optionally renders coarse Bevy
geometry for review.

## Current boundary

`BuildingProgram` describes an archetype, footprint, storeys, requested room
functions, construction family, timber-frame system, storey projection, and
roof pitch. `generate` produces:

- connected grid-cell regions with explicit room identities;
- one canonical wall at each exterior or inter-room boundary;
- a connected interior-door graph plus exterior doors, gates, windows, and
  arrow slits;
- stable-ID polygonal roof assemblies resolved from editable roof recipes,
  with typed ridges, hips, valleys, eaves, verges, abutments and opening cuts;
- grid-anchored round tower modules with integral-cell diameters, plus straight
  or helical vertical connectors; and
- eight distinct defensive crowns, continuous wall walks, tower-top decks,
  detached curtain walls, square bell towers, and corbelled corner bartizans.

`generate` is also the public validity boundary. An `Ok(BuildingPlan)` has
passed the complete semantic and geometric audit; a recipe that cannot produce
an audit-clean building returns a typed `GenerationError::StructuralContract`
containing the audit findings. Internal unchecked construction exists only so
the generator's mutation tests can prove that this boundary rejects corrupted
plans. The fixture seed matrix continuously exercises all archetypes at zero,
adjacent, ordinary proof, large, and wrapping-boundary seeds.

The grid is topological rather than voxel geometry. Floors, wall openings,
roofs, towers, stairs, and battlements are derived structures. Circular towers
therefore do not have to pretend that their circumference is a staircase of
square cells.

Ten curated programs exercise the current vocabulary:

- `town-house`: narrow, two-storey timber-frame house with a steep street gable;
- `hall-house`: broad hall plan beneath a steep half-hip roof;
- `fachwerk-cottage`: compact two-storey dwelling with close-studded timber
  framing and a different window rhythm from the merchant house;
- `fachwerk-merchant-house`: three projecting storeys, dense early-modern
  ornamental bracing, a street gable, cross-roof mass, and mixed dormers;
- `renaissance-town-hall`: a broad civic building with an intersecting
  half-hip and cross-gable roofscape, a transverse wall dormer, smaller roof
  dormers, and stepped or curved gable details;
- `cathedral`: an east-oriented, four-bay, three-aisled cruciform basilica with
  a projecting transept, square crossing, two-bay choir, five-sided apse, and
  an integrated single west bell tower;
- `castle-gatehouse`: gate passage, paired round towers, spiral stairs, arrow
  slits, bartizans, a projecting machicolated gallery, localized bretèche, and
  an open timber hoarding; and
- `courtyard-castle`: four wings around an open court, four corner towers,
  multiple roof pieces and dormers, permanent stone parapets, and unroofed
  fighting towers; and
- `walled-keep`: a detached central keep inside a gated outer curtain with
  four accessible corner towers and fighting platforms on both defensive
  layers; and
- `artillery-rondel-castle`: a retained medieval keep inside an earth-backed
  1544 retrofit with four rondels, a dry ditch, and a deniable bridge.

## Interactive editor

The native viewer can edit the same high-level authority used by generation:

```powershell
cargo run -p adventuresim-building-generator --bin building-viewer -- `
  --editor --document building-document.json
```

Middle-drag orbits, Shift+middle-drag pans, the wheel zooms, and `F` frames the
current selection. Resolved walls, openings, and timber framing map back to
stable grid selectors; hovering uses a grey outline and selection a white one.
The property window can add or remove wall openings, change eligible civilian
wall finishes, and change a timber-frame program. The **Fixtures** menu lists
every curated fixture. Switching fixtures replaces the building while keeping
the current camera, lighting, and editor scene in place. If neither a fixture
nor an existing document is supplied at launch, the editor starts with the
town-house fixture.

The editor uses a build-mode shell: a mode strip exposes Select, Construct,
Openings, Roof, Site, and Finish, while a storey rail presents the current
storey and the planned wall/roof visibility states. `1`–`6`, `Esc`, `Page Up`,
`Page Down`, `Home`, `R`, `Ctrl+Z`, and `Ctrl+Y` mirror those visible controls.
The current `BuildingDocument` remains the strict procedural-programme path:
only Select, Openings, and Finish activate an audited edit today. Construct,
Roof, and Site intentionally explain that they need the future freeform
player-build document instead of accepting an edit that cannot be saved or
rendered.

`BuildingDocument` is versioned JSON containing a `BuildingProgram` plus an
ordered edit log. Each UI command regenerates the complete plan and runs the
same audit as `generate`; an invalid command reports an error and leaves the
current document and scene unchanged. Undo and redo operate on document
snapshots. Save and load never serialize resolved meshes, which remain derived
evidence rather than a parallel editing authority.

The timber renderer treats *Fachwerk* as a structural system rather than a
painted facade. Its three patterns can place continuous sills and wall plates,
posts and close studs, horizontal rails, long diagonal braces, K-like braces,
Andreaskreuze, and the four-brace Mann figure. Upper storeys can project beyond
the wall below on visible timber brackets. Gable triangles receive their own
tie beams, king posts, collar beams, vertical studs, and outward braces.
Window-bearing bays are generated separately: their rails align with the sill
and lintel, structural studs flank the opening, and short braces stay in the
panels above or below rather than crossing the glazing.

Civilian windows are real wall openings with thin glazing recessed behind the
outer wall face. Separate jamb, sill, lintel, mullion, and transom meshes make
the depth and subdivision legible. Doors and gates are similarly recessed.
The curated castle fixtures instead treat their exposed exterior envelope as
defensive: ordinary apertures are narrow firing loops with no glazing. Flat
walls are split around the loop, round-tower shells omit surface facets at the
loop positions, and a darker inner embrasure surface sits behind the opening.
Pierced merlons and gun-loop parapets remain legacy, unaccepted vocabulary
until the resolver can prove their through-openings; curated military fixtures
use the ordinary masonry crown rather than silently substituting geometry.

The defensive vocabulary currently comprises ordinary crenellation, pierced
merlons, projecting masonry machicolation, open and roofed timber hoardings,
covered wall walks, continuous gun-loop parapets, localized bretèches, and
roofed or open bartizans. These remain semantic plan objects; the renderer does
not collapse them into a generic decorative crenel strip. Curated projected
defenses additionally resolve their host masonry, sockets, portals, support
bonds, floors, and drains into the renderer-independent geometry layer.

Every full battlement run now has an explicit 1.25-metre wall-walk surface on
the protected side of its parapet. Battlemented round towers have annular top
decks with open stair wells, and their spiral stairs rise to deck level. The
plan also records each constructed junction between fighting surfaces as a
level landing or short flight of steps, including its usable width and
headroom. Fighting surfaces are assigned exactly once to named defensive
circuits: the courtyard has one circuit, while the walled keep distinguishes
its outer curtain from the separately accessed inner keep. Round-tower graph
edges require explicit wall-walk portals. The renderer cuts those openings
through the tower shell and crown, adds landings, and gives every spiral stair
a framed ground entrance. The dark inner cylinder visible through loops and
portals is explicitly a non-colliding depth backdrop; the cut outer shell and
portal data define the physical opening. The viewer renders the surfaces as continuous
structural slabs. They
are suitable inputs for a future tactical collision or navigation adapter, but
this standalone prototype does not itself make agents pathfind across them.

The generator runs a semantic military-structure audit over every curated
plan. It rejects inward-facing parapets, unsupported battlements, missing or
undersized wall walks, roof-obstructed tower crowns, tower decks without stair
access, missing physical tower portals, multiply assigned or internally
disconnected fighting circuits, vertical discontinuities without usable steps,
narrow or low junctions, roof intrusions over wall walks, and curtain walls
without outward-facing parapets. Screenshot
manifests include the result and fail validation when the plan audit fails.
The renderer also audits geometry declared to be a closed solid: quantized
triangle edges expose boundary holes, non-manifold edges, inconsistent winding
(a common cause of visible backfaces), and degenerate triangles. Purposefully
open architectural surfaces remain classified separately until they acquire
volumetric construction.

These are coarse structural studies, not finished historical reconstructions.
The generator does not yet solve arbitrary polygon roofs with a general
straight-skeleton implementation, wall damage, navigation, or construction
chronology. Curated rectangles, L/courtyard compositions, dormers,
facade-derived cross gables, towers and cathedral aisle/nave hierarchies are
resolved into clipped face graphs with explicit framing, support contacts,
weathering and drainage. Pitch handles recompute those graphs under a declared
pivot policy and reject topology changes they cannot preserve.

## Captures

Render an exterior:

```powershell
just building-capture town-house exterior target/building-captures/town-house-exterior.png
```

Render a cutaway that exposes rooms and stairs:

```powershell
just building-capture castle-gatehouse cutaway target/building-captures/castle-gatehouse-cutaway.png
```

Audit rear and side defensive crowns from an elevated angle:

```powershell
just building-capture courtyard-castle defenses target/building-captures/courtyard-defenses.png
```

The military fixtures also provide deterministic proof views for the gate and
tower circulation contracts:

```powershell
just building-capture walled-keep gate-detail-exterior target/building-captures/walled-keep-gate-detail-exterior.png
just building-capture walled-keep gate-detail-interior target/building-captures/walled-keep-gate-detail-interior.png
just building-capture courtyard-castle tower-portal-detail target/building-captures/courtyard-tower-portal-detail.png
```

Each PNG is accompanied by a `.plan.json` containing the complete generated
recipe and a `.capture.json` describing what the screenshot was meant to show,
including focused component indices, daylight settings, luminance separation,
and clipping checks. Opaque architectural materials and the ground use a
shadowed oblique key with cool ambient fill; diagnostic void material alone is
unlit. The gate interior uses the same rig from the open section side so its
mechanism remains inspectable.
The viewer performs a disposable readback before the recorded screenshot so a
camera transition cannot be mistaken for the requested view.
After producing the nine crown proof files, validate that they all came from
one source revision and dirty-source fingerprint and that every view of a
fixture has identical plan and resolved-geometry hashes:

```powershell
target\debug\building-viewer.exe --validate-crown-suite target/building-captures
```

The Stage 4 roof proof matrix additionally validates 50 focused roof views and
all nine full-building regressions. Roof faces, child cuts, edge treatments,
supports, wall/tower contact contours, gutters and outlets retain exact IDs and
renderer fingerprints. Cathedral aisle sheds terminate against authoritative
clerestory wall faces, while the bell tower cuts and flashes the parent nave
roof rather than overlapping it as an independent solid.

Roof drainage deliberately uses a bounded MVP vocabulary. Adjacent face
catchments feed shared perimeter or ring stations, with no more than four
stations per roof assembly. A station either reaches a supported downspout on
an opening-free planar facade, free-drips vertically onto a named point that
is contained by and lies on an exact parent-roof face, or free-drips to a named
ground splash area when no safe facade host exists. Parent-roof free-drip is a
narrow child-eave case, not a general arbitrary recipient solver; intermediate
roof hits or solids reject it. Ground free-drip sweeps its fall and splash clear
of architecture and circulation.
The audit rejects pipes crossing openings, portals, stairs, defensive walks, or
accessible tower stages. Decorative leader heads, buried drainage, and radial
tower plumbing are deferred until they provide more architectural value than
their additional topology and clearance cases; the prototype does not emulate
them with detached rods or one pipe per roof facet.

```powershell
target\debug\building-viewer.exe --validate-roof-suite target/building-captures
```

The Stage 5 church program makes the cathedral's bay system authoritative
rather than inferring it from a picturesque roof shell. Each nave axis owns
paired piers, arcades, buttresses, clerestory lights, vault shells, and load
surfaces. The crossing, choir, radial apse supports, west portal, nave passage,
guarded spiral, bearing-ring bell floor, bell frame, unglazed louvres, and roof
service ladder retain stable resolved IDs and grounded support paths. The
westwork floor is borne by the tower-wall nodes; it is not declared grounded
merely because it occurs inside the tower.

Thirty deterministic church views cover the whole building, a representative
nave bay, crossing, choir/apse, bell tower, drainage, and support graph. Validate
that they share one current program and resolved geometry authority with:

```powershell
target\debug\building-viewer.exe --validate-church-suite target/building-captures/stage5-church-proof-v1
```

This is intentionally a coarse structural type, not a universal cathedral
grammar. Ornate tracery, sculpture, pinnacles, carved portals, detailed rib
profiles and bosses, crypts, chapels, sacristies, furnishings, acoustics, brick
textures, and underground drainage remain explicitly deferred. They are not
represented by decorative witnesses or semantic booleans.

## Research decisions

The first roof iteration uses editable roof pieces rather than immediately
attempting a general roof solver. This follows the useful interaction boundary
of *The Sims 4*: rooms, walls, stairs, and roofs remain independently movable
architectural elements. EA also described an experimental automatic-door
placement pass, supporting the separation between semantic layout and later
opening placement:

- [EA: early concepts from The Sims 4](https://www.ea.com/news/see-early-concept-art-from-the-sims-4)
- [Maxis Build Mode design summary](https://simscommunity.info/2014/06/05/building-anticipation-for-the-sims-4/)

The room allocator follows a data-first room-graph approach: seed requested
functions, expand only through adjacent unclaimed cells, derive shared
boundaries, and then select a spanning set of interior doors. A later arbitrary
polygon roof should use a weighted straight skeleton, whose wavefront directly
produces roof ridges and supports different edge speeds or pitches:

- [Aichholzer et al.: A Novel Type of Skeleton for Polygons](https://www.jucs.org/jucs_1_12/a_novel_type_of/Aichholzer_O.pdf)
- [Weighted straight skeletons for roofs and terrains](https://arxiv.org/abs/1604.03362)
- [Dungeon Alchemist straight-skeleton implementation notes](https://github.com/Briganti-Games/Straight-Skeleton-Generator)

The fixture vocabulary deliberately emphasizes forms visible in German lands
around the game's 1544 setting: steep roof masses and prominent gables,
irregular castle building groups, round or polygonal stair towers, and exterior
spiral stairs as status-bearing circulation. Defensive projections distinguish
ordinary crenellation from machicolation: the latter has an overhanging gallery
and corbels so openings can address the wall foot.

- [Göttingen Academy: large-scale structure of late-medieval and Renaissance residences](https://adw-goe.de/cs/digitale-bibliothek/hoefe-und-residenzen-im-spaetmittelalterlichen-reich/id/rf15_II_121207-958/)
- [Göttingen Academy: spiral stairs and stair towers](https://adw-goe.de/cs/digitale-bibliothek/hoefe-und-residenzen-im-spaetmittelalterlichen-reich/id/rf15_II_121207-1006/)
- [Schloss Hartenfels: the 1533-1537 Great Spiral Staircase](https://www.schloss-hartenfels.de/en/nav-main/exploring-the-castle/the-big-spiral-staircase)
- [Prague Institute: tall and stepped Renaissance gables](https://staletapraha.cz/en/artkey/pha-201802-0003_the-roof-architecture-and-the-renaissance-make-up-of-prague-towns-during-the-reign-of-the-king-and-emperor-ferd.php)

The expanded civilian pass uses *Fachwerk* terminology conservatively. Posts,
sills, plates, rails, and braces are load-bearing members; an Andreaskreuz is an
X-brace, while a Mann figure combines head and foot braces around a post. The
1544 fixtures use late-medieval and early-modern systems, including
storey-by-storey construction and projection, without treating modern tourist
labels as rigid regional or ethnic categories:

- [BauNetz Wissen: Fachwerk construction and member names](https://www.baunetzwissen.de/holz/fachwissen/holzbausysteme/fachwerkbauweise-7820010)
- [Denkmalstiftung Baden-Württemberg: historical framing and decorative forms](https://denkmalstiftung-baden-wuerttemberg.de/wissen/baukunst/d-f-baukunst/fachwerk/)
- [Bietigheim-Bissingen City Museum: the 1535/36 Hornmoldhaus transition](https://stadtmuseum.bietig-bissingen.de/hornmoldhaus-museum/geschichte-des-hornmoldhauses/architektur-des-fachwerkhauses/)

Transverse wall dormers are modeled separately from ordinary dormers because a
Zwerchhaus continues the facade and carries a roof perpendicular to the main
ridge. Both forms belong in late-medieval and Renaissance roofscapes:

- [BauNetz Wissen: Zwerchhaus](https://www.baunetzwissen.de/glossar/z/zwerchhaus-1153505)
- [BauNetz Wissen: historical dormers](https://www.baunetzwissen.de/bauen-im-bestand/fachwissen/dach-konstruktion/historische-dachgauben-3010573)

Defensive crowns distinguish function and material. Hoardings project in
timber; machicolations replace that vulnerable gallery with masonry on corbels;
a bretèche protects a limited point such as a gate; and a bartizan is a small
overhanging turret rather than a continuous parapet:

- [World History Encyclopedia: illustrated castle-architecture glossary](https://www.worldhistory.org/article/1233/an-illustrated-glossary-of-castle-architecture/)
- [Muralla de Ávila: defensive-wall element glossary](https://muralladeavila.com/en/what-do-you-know-about-the-walls/what-is-each-part-called)

The absence of glass is a property of these defensive loops, not a universal
rule for every castle room. Residential ranges could have large windows, but
those openings weakened defense; the fixtures currently represent the exposed
gatehouse and curtain-wall condition. Firing loops stay narrow outside and
open into a deeper interior embrasure so a defender can aim from cover:

- [English Heritage: Restormel Castle arrowloops and vulnerable large windows](https://production.english-heritage.org.uk/visit/places/restormel-castle/history/description/)
- [Canterbury Historical and Archaeological Society: arrow-loop definition](https://www.canterbury-archaeology.org.uk/arrow-loop)

The current regular courtyard castle is one valid late-Renaissance program,
not the assumed universal castle plan. Contemporary German residences often
retained inherited, irregular building groups; future programs should add
incremental accretion rather than merely varying a symmetric four-wing seed.
Its current military crown is intentionally one continuous permanent masonry
family: ordinary crenellation runs around the curtain and all four towers,
while special protection remains localized at the gate. Pierced merlons are
deferred until their openings can be expressed as true resolved voids rather
than painted recesses. Each accepted masonry crown is an authoritative resolved
assembly with a 0.90-metre breastwork below every crenel, a 1.70-metre merlon
top, coping, open scuppers, protected inner walk edge, stance and firing lines,
and owned corner/tower splices. The walk itself is resolved as supported,
outward-sloped catchment solids and drainage surfaces. Each walk slab stops at
a 0.12-metre exposed-edge slot instead of occupying the channel volume. A
separate recessed channel floor begins below the adjacent toe, descends
longitudinally through an obstacle-checked segment chain, and turns through the
breastwork only inside the open scupper. The audit rejects a raised channel or
an uncut walk slab, then samples the full inner,
middle, and outer walk width, verifies downhill reachability, and covers all
four straight-wall orientations plus 144 radial tower angles. Round deck chords
overlap at their inner edge and are sized from the outer sector boundary so no
triangular foot gaps remain. The exact 60-millimetre crossfall,
18-millimetre channel fall, and 0.12-metre slot are prototype drainage/readability gates, not
universal historical dimensions. Tower splices resolve explicit return pieces
whose measured contact area, gap and penetration must fit the local bond prism;
the audit rejects displaced and over-penetrated bonds. These exact envelope dimensions are prototype
gameplay gates, not universal historical measurements. The same deterministic
resolved-solid IDs, support nodes, drainage routes, voids and dimensions feed
auditing and rendering. Capture manifests bind the resolver schema, exact
per-item render multiset, source revision and dirty-source fingerprint, resolved
geometry hash, and plan/evidence hashes so stale or transformed proof cannot
silently pass.
The roof eaves are held
behind the fighting circuit rather than occupying its headroom. The detached
walled-keep fixture uses a separate coherent outer crenellation and inner
gun-loop parapet, a 1.2-metre inferred prototype minimum for curtain and tower
masonry, and a pair of close flanking towers at the gate. Two modeled, splayed
firing apertures must geometrically cover both the gate threshold and approach
within their arcs and ranges. The three-dimensional segment audit rejects
intervening curtains, tower shells, building walls, gate-chamber structure,
closures, roofs, decks, and wall walks after subtracting the originating
aperture. Heavy gate leaves and a second portcullis closure are operated from an
explicit supported guard chamber: its floor, walls, observation and downward
openings, windlass position, and stair connection to the wall walk are modeled
and rendered rather than represented by a capability flag. The courtyard
towers use the same minimum fortified shell profile. These numeric thickness,
clearance, chamber-area, arc, and range gates are game-design inferences for
declared defensive profiles, not universal historical rules.

The walled-keep gate is authored as one cardinal, wall-local
`GatehouseAssemblySpec`, not as independently positioned cylinders and boxes.
Its integral-cell tower diameter and parity-checked lattice anchors derive the
symmetric flanking towers, chord-cut bonded interfaces, positive-area curtain
returns, clear gate passage, segmental masonry arch and spandrel bearings,
guard-chamber volume, closures, access, openings, and firing apertures in any
of the four wall orientations. Metre-valued plans are resolved output caches;
the structural audit rejects cache drift, blocked passage or room voids,
missing bearing paths, unmatched round-to-rectangular splices, unresolved
apertures, and undeclared overlaps using resolved clear prisms, solid prisms,
and chord-cut cylinder tests. Even-cell tower centres must occupy room-grid
vertices and odd-cell tower centres must occupy cell centres; invalid numeric
values are rejected during construction and deserialization. The 1/30-cell structural lattice and exact
module dimensions are prototype construction gates, not historical claims.
The gate passage also carries an explicit rectangular-plus-segmental-arch
cross-section. Both the heavy plank leaves and portcullis derive their local
top height from that profile, and the audit rejects any closure line that
leaves an unsecured rectangular strip or arched lunette.

Guard-chamber circulation uses a protected-side military service stair rather
than an internal flight squeezed beneath the chamber roof. Its authoritative
route contains a positively overlapping wall-walk landing, a roof-clearance
slot and upper wall opening, a supported 10-riser exterior flight with guards,
a full bottom landing, and a floor-threshold rear door with inward swing
clearance. Every exposed landing-boundary interval carries an explicit guard;
the wall-walk, stair, and door connection intervals remain open. A masonry
ledger plus transverse and longitudinal diagonal braces give the tall timber
posts a declared lateral load path. The 0.90 m traversal width, 1.90 m swept
height, 0.12–0.19 m riser,
0.25–0.34 m going, 38-degree maximum pitch, landing depth, nosing, and guard
dimensions are game/animation gates rather than historical universal values.
The audit sweeps the route against the roof and wall openings, tower solids,
murder hole, windlass operating clearance, and door swing, and verifies the
stair and landings have both vertical and lateral support paths.

Projected defenses are now resolved assemblies rather than battlement enum
decorations. Permanent masonry machicolations and localized bretèches, stored
or deployed temporary timber hoardings, and hollow usable bartizans each own
their floor pieces, downward throat voids, wall-foot rays, protected outer
wall, access portal and landing, support graph, firing apertures, roof and
independent drain where exposed. Each assembly records the exact top-storey
exterior wall-cell IDs it replaces. The resolver rebuilds those source cells
inside their original vertical envelope, splits them around real access and
socket voids, and the renderer suppresses the corresponding legacy wall cells,
so a defense cannot add a freestanding witness screen above the building.
Host wall walks overlap the access landing, and a positive-area junction bond
connects the projection supports to grounded masonry. Threatened-wall
bartizans additionally bear on an explicit grounded, wall-bonded buttress
rather than claiming support from one flat witness face. Hoarding sockets are host-masonry
voids; the stored state renders the open holes, while deployed joists occupy
and embed into those same socket IDs. No non-colliding witness wall participates
in audit or proof rendering.

Floor solids stop around every throat. Bartizan floors remain continuous
around their declared throat and access voids, while firing loops split only
the narrow lower, upper, and jamb portions of nearby shell facets instead of
removing full-height sides. Supported working positions produce independent
near, middle, and far rays from the real throat or loop plane; the audit rejects
below-floor origins and rays crossing friendly circulation. Gallery floors and
roofed works carry physical slope, feed lower disjoint channels, and reach
exact open outlets through sampled downhill paths. Lean-to roofs also have a
resolved high-edge flashing at the host junction so that edge cannot become a
trapped valley. Open masonry works use sloped, overhanging coping with outward
drips; round bartizan coping resolves and drains each shell facet independently. The
localized bretèche roof seats on two explicit wall plates: its outer enclosure
rises to the low plate, while paired rear posts carry the high plate through the
gallery support graph. The audit samples both bearing lines and rejects a raised
roof or missing, shortened, or displaced support.
audit rejects a gallery slab laid through the opening, a blocked or inward ray,
unsupported corbel/frame bearings, dangling timber, inaccessible or cramped
walks, flat, raised, reversed, or trapped drainage, closed bartizans, and
incoherent material/phase or tactical-target declarations. The 0.90-metre
route, 1.90-metre headroom,
0.75-metre doorway pinch, 0.08-square-metre minimum support bearing,
0.75-metre floor-to-support spacing and 1.20-metre timber cantilever are
prototype game and animation gates, not historical universal dimensions.

The curated gatehouse does not display every device as a catalogue. Its main
state has permanent gate-approach machicolation and sockets showing capacity
for a temporary siege-front hoarding. Deterministic isolated study states place
a bretèche at one named threatened wall foot, a deployed campaign hoarding on
another front, or bartizans at threatened corners while retaining the accepted
ordinary host crown and circulation. Capture manifests identify the exact
assembly solids, voids, ray count, deployment and tactical target; the
projected-defense suite rejects mixed source builds, stale fixture/seed hashes,
or missing proof IDs.
