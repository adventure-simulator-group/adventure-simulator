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
- intersecting parametric roof pieces, wall dormers, roof dormers, and explicit
  gable profiles;
- round tower modules and straight or helical vertical connectors; and
- eight distinct defensive crowns, continuous wall walks, tower-top decks,
  and corbelled corner bartizans.

The grid is topological rather than voxel geometry. Floors, wall openings,
roofs, towers, stairs, and battlements are derived structures. Circular towers
therefore do not have to pretend that their circumference is a staircase of
square cells.

Six curated programs exercise the current vocabulary:

- `town-house`: narrow, two-storey timber-frame house with a steep street gable;
- `hall-house`: broad hall plan beneath a steep half-hip roof;
- `fachwerk-merchant-house`: three projecting storeys, dense early-modern
  ornamental bracing, a street gable, cross-roof mass, and mixed dormers;
- `renaissance-town-hall`: a broad civic building with an intersecting
  half-hip and cross-gable roofscape, a transverse wall dormer, smaller roof
  dormers, and stepped or curved gable details;
- `castle-gatehouse`: gate passage, paired round towers, spiral stairs, arrow
  slits, bartizans, a projecting machicolated gallery, localized bretèche, and
  an open timber hoarding; and
- `courtyard-castle`: four wings around an open court, four corner towers,
  multiple roof pieces and dormers, and the complete defensive-crown sample.

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
Pierced merlons and gun-loop parapets are also assembled around empty slots
rather than receiving dark or blue decals.

The defensive vocabulary currently comprises ordinary crenellation, pierced
merlons, projecting masonry machicolation, open and roofed timber hoardings,
covered wall walks, continuous gun-loop parapets, localized bretèches, and
roofed or open bartizans. These remain semantic plan objects; the renderer does
not collapse them into a generic decorative crenel strip.

Every full battlement run now has an explicit 1.25-metre wall-walk surface on
the protected side of its parapet. Battlemented round towers have annular top
decks with open stair wells, and their spiral stairs rise to deck level. The
viewer renders these as continuous structural slabs. They are suitable inputs
for a future tactical collision or navigation adapter, but this standalone
prototype does not itself make agents pathfind across them.

These are coarse structural studies, not finished historical reconstructions.
The generator does not yet solve arbitrary polygon roofs with a general
straight-skeleton implementation, structural loads, wall damage, navigation,
or construction chronology. Complex roofs in this prototype are bounded
compositions of intersecting masses and attachments, which is enough to
represent cross roofs and period roofscapes while retaining direct editing.

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

Each PNG is accompanied by a `.plan.json` containing the complete generated
recipe and a `.capture.json` describing what the screenshot was meant to show.
The viewer performs a disposable readback before the recorded screenshot so a
camera transition cannot be mistaken for the requested view.

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
