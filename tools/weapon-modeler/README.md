# Parametric weapon modeler

This standalone browser tool experiments with modular, parameterized weapon
geometry, including melee weapons, shields, hand bows, and crossbows with independent ammunition and carriers. It is deliberately outside the Rust workspace and does not
participate in strategic or tactical builds.

Curved outlines and swept bars use shared adaptive sampling with explicit
maximum-chord and curve-deviation budgets. The sampler preserves authored
endpoints while discarding numerically redundant neighbors, and prism
triangulation removes only scale-negligible collinear vertices. This keeps
hooks, blade bellies, beaks, guards, rings, and mace flange profiles smooth
without introducing zero-area faces or weakening outline and tube-contact
validation. Cubic spans use co-directed, equal-length handles at intended
smooth joins; authored working points such as blade apices and hook tips remain
deliberate corners. Round swept bars select their cross-section tessellation
from 6 mm chord and 0.3 mm sagitta budgets, with an LOD-dependent floor and any
higher authored sampling request scaled by the selected detail level.

From this directory, run:

```powershell
npm test
npm start
```

Then open <http://127.0.0.1:4173>. The viewer has no package dependencies: the
local server uses Node's standard library and the renderer uses WebGL 2
directly.

Weapon presets are declarative graphs in `src/presets.js`. Shared generators in
`src/mesh.js` currently cover tapered shafts, sockets, grips, pommels, guards,
curved, fullered, and diamond-section blades, sampled axe heads, shaped hammer
polls, curved beaks, continuously forged fork/partisan/glaive heads, spear
points, smooth swept knuckle bows, side and finger rings, fan pommels, flanged mace
heads, langets, and butt caps. Geometry placement resolves through named frames
such as `weapon.root`, `shaft.top`, `grip.top`, `guard.center`, and
`blade.base`. A component can attach one of its local endpoints to a frame or
stretch between two frames. Consequently, changing a grip or shaft length
moves its dependent furniture and head instead of opening a gap. Optional
component Euler rotations allow the same local head to face left, right, or out
of plane.

The assembly composer independently combines either a wooden polearm shaft or
steel one-hand haft with mace, halberd, spear, hammer/pick, axe, armour beak,
fork, bill, glaive, and partisan head assemblies. Composed previews retain
editable haft and head controls addressed by stable component IDs rather than
array positions. It
uses the same frame resolver and mesh generators as the presets; socket and
sleeve dimensions derive from the selected haft radius. Socket and sleeve
profile radii mean outer radii; their inner contact envelope is the resolved
shaft radius and a separately controlled 2–6 mm wall. In particular, the
flanged-mace head can be mounted on the polearm shaft without a special mesh.

Slider changes, JSON edits, and composed previews are transactional. The tool
first builds a copy and validates strict per-kind component fields and types,
materials/mounts, integer tessellation minima, dimensions, nonempty part volume,
control contracts, mandatory parentage, rotation-aware transformed
parent-footprint contact, concentric axial heads, radial socket fit, simple
outlines and tube paths,
finite bounds, shared-renderer front/oblique projected-vertex camera fit,
triangle winding/normal agreement, and closed oriented two-manifold topology.
A valid candidate replaces the current model; an invalid
candidate is rejected with an actionable message and leaves the last good
model visible. The JSON editor therefore remains useful for direct experiments
and unusual head swaps without allowing a broken definition to enter the live
viewer.

The library contains weapon and shield presets spanning halberds, hammers,
spears, pikes, forks, bills, glaives, swords, daggers, axes, and maces. They are
reference-oriented silhouettes for rapid iteration, not claims of exact museum
reconstruction.

## Hand bows, arrows, and quivers

The hand-bow family is implemented by dedicated `archeryBow`, `arrow`, and
`arrowQuiver` components, exposed as independent bow, ammunition, and carrier
presets so an exported bow mesh and its mass never include floating display
accessories. The two bow endpoints are a tall Central European self-bow family
and a shorter reflex-recurve composite family. Controls cover total length,
upper/lower proportion, grip, limb width and depth, tip taper, mid-limb reflex,
tip recurve, brace height and string dimensions. Separate arrow and carrier
controls cover shaft/head/fletching/nock proportions and quiver length, taper,
wall, mouth binding, and shoulder strap. Discrete selectors expose D, oval, and
flat limbs; self or laminated construction; broadhead or bodkin points; self or
horn-reinforced nocks; fletching count/color; and rigid-quiver or soft-bag form.

A generated strung bow retains separate named meshes for the upper and lower
limbs, grip, tip overlays, upper and lower string control spans, center serving,
and closed string end-loops seated at the limb-tip nocks. The nocking center is
one straight served string with regular swept-mesh edge rings, not decorative
toroids. Each tip loop is built in the local limb-tip tangent frame and clears
the tapered nock cross-section instead of lying in the bow's global plane. In
skinned GLB output the bow body preserves the existing character-rig
attachment path, while those five string meshes become individually named rigid
children of the weapon joint under the `bow-string-nodes-v1` contract. Animators
can therefore select and transform spans, serving, and tip loops independently;
melee exports remain the original single skinned mesh. The arrow retains shaft,
head, individual fletchings, and a genuinely open string slot whose declared
maximum string radius and clearance are validated together. The quiver is a
manifold hollow shell with an uncapped mouth and a separately sealed bottom;
both strap anchors are evaluated against the tapered body profile and overlap
it intentionally rather than floating at the mouth radius.

Dimensional anchors are intentionally transparent. The [Mary Rose Museum's
longbow and arrow survey](https://maryrose.org/discover/collections/the-weaponry-of-the-mary-rose/longbows-and-arrows/)
records mid-sixteenth-century yew self bows at 1.839–2.113 m, mostly D-sectioned
at about 35 by 33 mm at the center, and arrows from 667–880 mm. The self-bow
default therefore uses a 36 by 32 mm D-section rather than a thin rectangular
strip; selectable oval and flat studies share the same dimensions. Composite
wood, horn, and sinew intervals use one taper-aware layout, meeting exactly at
every limb station without gaps or unintended overlap. Composite
construction and strongly reflexed/recurved geometry follow the Metropolitan
Museum's material account and measured examples in [Islamic Arms and Armor in
The Metropolitan Museum of Art](https://resources.metmuseum.org/resources/metpublications/pdf/Islamic_Arms_and_Armor_in_The_Metropolitan_Museum_of_Art.pdf): wood core, horn
belly, sinew back, curved end sections, and string loops at the nocks. The
composite preset represents a contemporary family encountered through Central
European and Ottoman contact, not a claim that its decorative treatment is a
specific German museum object.

## Crossbows, bolts, and bolt carriers

The `crossbow`, `crossbowBolt`, and `boltQuiver` components are independent
exportable assets. Three curated crossbow endpoints cover a heavy German
steel-prod hunting weapon prepared for a cranequin, a retained Central European
horn-wood-sinew composite arbalest with goat's-foot accommodation, and a compact
belt-hook/target family study. The compact endpoint is deliberately labeled as
a family study rather than a reconstruction of a dated 1544 German object.
Crossbow controls cover tiller length and straight, waisted, or swollen plan;
vertical butt drop, lock-table height, fore-end rise and optional staghorn facing;
prod span, depth, thickness, sweep and taper; steel or layered composite
construction; draw/nut position; string, serving and tip loops; bridle spacing;
split rotating nut cheeks around a real string notch, axle/bearing, bolt-butt
shelf, sear and connected long trigger; paired recessed runner rails; stirrup; cranequin, goat's-foot, or
belt-hook spanning accommodation; and optional peep/post furniture.

The tiller is a combined plan-and-side-profile loft, not a uniformly extruded
slab: butt, waist, and fore-end widths own separate stations while butt drop,
lock-table depth, and fore-end rise own the vertical profile. Rear and fore-end
stock bodies stop on either side of the lock; two narrow bearing cheeks bridge
them outside an omitted central volume. That omission is the open nut cavity—no
decorative wood "well" is layered inside it. The paired runner rails touch the
fore-end at a shared zero-height bearing datum without entering its volume;
nut, string, butt shelf, axle, sear, and trigger use the same datum.

Every crossbow keeps its tiller, prod/layers, two prod bridles, release nut,
trigger, bolt groove, stirrup, and chosen spanning interface as named manifold
parts. Its single continuous working string is represented by separately named
left and right control spans, a served nocking span, and closed end loops seated
in each prod tip's local tangent frame. In skinned GLB output these five string
parts are independent rigid children of the weapon joint under the
`crossbow-string-nodes-v1` animation contract, while the remaining weapon stays
on the existing skinned export path. Bow and melee export contracts are
unchanged.

The bolt preset independently varies length, shaft radius, head dimensions,
and bodkin/broadhead/hunting form. It is a quarrel rather than an arrow: its
flattened horn-reinforced butt bears on the paired runner and nut shelf directly
against the served string, with no arrow-style nock. War quarrels carry two
angled stiff leather/wood vanes; hunting bolts carry three feathers. Spanning
furniture is likewise functional rather than symbolic: cranequin weapons have
a transverse stock-rest peg and rack purchase rail, goat's-foot weapons have
paired pivot lugs and axle, and the comparative belt-hook weapon has a broad
underside purchase bar. Each preset exposes only its applicable prod, spanning,
and sight choices.

The bolt carrier is not the round arrow-quiver generator. It is a dedicated
broad, tapered, open-mouthed construction based on Met 29.158.646a-l, with
separate wood front/back/side shells, paper lining, hide cover, leather mouth
binding, sealed wood base, and an attached shoulder strap. Its default is 44.6
cm high and 29 cm across the broad bottom and generates approximately 0.46 kg,
close to the catalog's roughly 448 g record.

Dimensional anchors come from the Metropolitan Museum's catalog and [*A Deadly
Art: European Crossbows, 1250–1850*](https://www.metmuseum.org/met-publications/a-deadly-art-european-crossbows-1250-1850).
The early-sixteenth-century southern German steel crossbow 14.25.1572a is
recorded at 73.7 cm long, 62.4 cm wide, and 3 kg. The heavy default defines
overall length as tiller butt through the outside of the modeled foot stirrup;
its 61.2 cm tiller plus 12 cm stirrup generates about 73.5 cm overall, while
the prod-tip center span is 62.4 cm and calculated construction mass is about
2.85 kg (within the object's approximate 3 kg construction target). The mechanisms and furniture also follow the Museum's
[later German/Saxon crossbow and cranequin 14.25.3383a-c](https://www.metmuseum.org/art/collection/search/33739):
walnut tiller, steel prod lashed with hemp, rotating nut, bolt-butt notch,
long trigger, safety/sight furniture, transverse cranequin rest, and the
documented distinction between cranequin spanning and light belt-hook or
goat's-foot weapons. That object is later (ca. 1575-1650) and is used only as a
construction/mechanism reference, not passed off as a 1544 specimen. Bolt and
carrier proportions follow the early-sixteenth-century German and Central
European bolt/quiver records cataloged in *A Deadly Art*; the generator exposes
their meaningful construction choices instead of claiming one exact object
replica.

## Firearms, lead balls, and ball pouches

The `firearm`, `leadBall`, and `ballPouch` components are independent small-arm
outputs; the generator does not model artillery, loose powder, powder flasks,
or priming charges. The firearm family spans a Munich double-barreled
wheellock pistol, a German matchlock shoulder arm, and an honestly labeled
single-barrel wheellock family study. Family selectors deliberately constrain
barrel count, lock type, and stock form to coherent combinations rather than
offering an ahistorical matchlock pistol or double-barreled arquebus.

Firearm controls cover overall and primary/secondary barrel length, bore and wall thickness,
octagonal breech share, ringed or plain muzzle, butt/lock/fore-end widths,
stock depth and drop, wheel or pivot size, pan and trigger dimensions, ramrod,
sights, and optional staghorn facing. The stock is a combined plan-and-vertical
profile loft: butt, lock waist, and fore-stock widths materially alter separate
stations, while the shallow swept cherry pistol stock has an attached solid
spiral-fluted bulb pommel and the walnut/red-beech matchlock has a broad cheek
stock. Shaped staghorn or bone side plaques, mother-of-pearl/staghorn inlays,
and latten/brass/gilt-steel furniture are placed as legible construction rather
than one generic slab. Each barrel is a manifold hollow tube with an open muzzle, joined
octagonal and round stages, and a separately sealed breech. Bands, sights,
ramrod, trigger guard, and decorative facing remain named construction parts.
Bands are closed XZ enclosures perpendicular to the bore; the trigger guard is
a YZ side-elevation loop enclosing its reachable trigger blade.

The Peck wheellock assembly carries two complete ignition trains on one lock
plate. Each has its own wheel, axle/bearing, mainspring, cock arm, split jaws
holding visible pyrite, open-topped pan cavity, hinged cover, and touchhole to
its bore. Touchhole centerlines cross the full barrel wall and end slightly
inside the bore radius rather than stopping on the outer barrel surface; the
trigger blade, sear linkage, and safety provide the shared release
path. The matchlock instead provides a pivoted serpentine, split jaws holding a
visible match, open pan, hinged cover, touchhole, linkage, and trigger.
Moving parts export as individually named rigid children of the weapon joint
under `firearm-lock-nodes-v2`. Each moving mesh is recentered on an explicit
local pivot, and its GLB node translation places that pivot in weapon space, so
wheel, cock, cover, safety, trigger, and serpentine rotations do not orbit the
weapon origin. The stock, barrels, and static furniture retain the existing
skinned weapon export. Bow, crossbow, and melee contracts remain unchanged.

The independent lead ball has exactly one editable value, radius. Compatible
ball diameter is always smaller than bore diameter; that positive windage is
documented and cross-preset tested rather than silently forcing the projectile
to fill the bore. The independent leather ball pouch has a sealed body, an open
mouth, a bounded 0–120 degree hinge-local flap, selectable toggle or buckle
closure, and two attached belt loops. The flap is a separate GLB child under
`pouch-flap-node-v1`, closes across the mouth onto the front, and rotates clear
for access. It contains no powder-related geometry. Low-detail round balls keep
enough latitude/longitude rings to remain within eight percent of analytic
sphere volume without adding any projectile control besides radius.

The dimensional endpoints come directly from two Metropolitan Museum records.
Peter Peck and Ambrosius Gemlich's [double-barreled wheellock pistol for Charles
V, 14.25.1425](https://www.metmuseum.org/art/collection/search/22387) is Munich
work of about 1540–45, 49.2 cm overall with vertically stacked unequal barrels:
25.4 cm upper, 19.4 cm lower, both 11.7 mm caliber, in steel, gold, cherry wood,
and staghorn. The Museum's [German sixteenth-century matchlock gun,
28.100.6](https://www.metmuseum.org/art/collection/search/34811) is 160.3 cm
overall with a 121.6 cm barrel, 17.7 mm caliber, and recorded mass of 6.15 kg.
The default generated arquebus preserves those dimensions and calculates about
6.10 kg from the modeled walnut/red-beech, steel, latten, bone,
mother-of-pearl, and brass construction. The separate single-barrel
pistol is a comparative generator endpoint, not a claimed reconstruction of a
specific dated object.

The two flanged-mace presets are endpoints of one generator rather than
separate meshes. Its controls interpolate between a compact central-cusp head
and an elongated Gothic head with high cusped shoulders and pointed crown,
while also varying flange count, core and shoulder radii, concavity, steel
haft, dark grip, and collar dimensions.

The other shared head families expose comparable construction parameters rather
than only overall scale. Axe controls include reach, height, plate depth, eye,
shoulder blends, flare, toe, heel, beard, curvature, and side. Spear and
partisan controls cover length/width, root, belly position, point acuteness,
section depth, and partisan lugs. Hammer polls and armour beaks expose their
neck, face, crown, root/tip sections, bend, set, and depth. Glaive, bill, and
fork controls cover their working contours, points, tangs, hooks, crotch, tine
taper, and shoulder transitions. Socket and linked langet dimensions are also
editable. Useful ranges are constrained, and dependent measurements such as a
poll neck are clamped by the generator so combined slider extremes remain
simple and non-self-intersecting.

The Dussack and primitive bearded-axe entries are explicitly marked as
non-curated generator studies pending tighter object references. Curated head
presets carry constrained reference-scale breadths; for example, the default
German halberd is 25.5 cm across, using the Metropolitan Museum's circa
1525–1550 German halberd (24.1 cm recorded width) as its dimensional anchor.

This is an asset-development experiment, not authoritative gameplay code.
The viewer measures enclosed mesh volume, material-weighted mass, center of
mass and moment about the grip. Fitted sockets and bosses are hollow shells;
metal bucklers use thin plate, and wooden shields use leather edge binding.
Mass remains a construction diagnostic: overlapping assembled parts, material
simplifications and missing fasteners prevent museum-level mass calibration.
The animator export preserves indexed geometry and the selected LOD in a
skinned GLB. UVs, texture-space tangents, normal maps and collision geometry are
not generated by this tool.

`npm test` includes deterministic seeded parameter sweeps. Every preset control
is exercised at its default, minimum, maximum, random values, adjacent pairs,
multi-control combinations, and every four-way pair of endpoints (more than
9,000 pair cases). The same structural validator covers every haft/head
composition and its editable controls, including pairwise endpoints. Regressions
cover dependency propagation such as the Großes Messer grip moving its guard,
blade, and Nagel together, shaft/socket fit, detached offsets, invalid schemas,
crossing outlines/tubes, typed nested attachment values, restricted
two-endpoint knuckle-bow stretching, and hammer-control geometry changes.


## Construction, shading and detail

Round grips, lathes, swept bars, bosses and shield surfaces share vertex indices
and angle-weighted normals within their construction surfaces. End caps,
blade ridges/cutting edges and authored octagonal haft flats retain split
vertices. The renderer uses indexed draws and GLB exports retain those indices
and normals. Explicit surfaces and sharp-corner splitting prevent a universal
smoothing pass from rounding over working edges.

Low, medium and high detail share sampling budgets for round sections, curves,
blade stations, shield surfaces, rims and fittings. Narrow shield ribs impose a
curvature-based resolution floor. LOD changes tessellation while preserving the
authored dimensions, attachment frames and intentional facets. Straight shafts
and planar faces do not receive unnecessary length subdivisions. Select **Mesh
detail** in the editor, or pass `--lod low|medium|high` to `cli.mjs`.

Sword grips taper into small pommel necks without an overhanging bottom cap.
The unified pommel component offers lathed bulb/pear/scent-stopper profiles,
wheel plates with beveled rims, intentional faceted buns, spirally fluted fig
forms, fan/fish-tail outlines, and composite bases with named ornament sockets.
The crown, escutcheon and indexed authored-relief examples demonstrate reusable
ornament modules; faces and animals can be supplied as authored indexed meshes.
Controls appear only for the selected construction. Representative defaults
show the wheel on the estoc, a fish-tail on the Messer, a writhen fig on the
two-handed sword, and a faceted bun on the riding sword.

Crossguards sweep round, oval, diamond, flat or triangular sections along
symmetric, opposed or independent left/right arms. Each independent arm has
length, sweep and out-of-plane set. Section roll produces twisted members;
parallel-transport frames avoid orientation flips on 3D paths. Ball, disk,
pyramidal, scroll, fish-tail and vase terminals remain independent of the
quillon section. The riding sword uses a connected named-node graph for its
side ring, finger loop and knuckle bow. The lower bow follows the grip-base
frame, and its middle node derives from the moving endpoints. An optional
later-style shell study demonstrates a dished, rolled-rim plate with a true
cutout; it is not part of the c.1540 default. The shell primitive currently
supports one matched outline/cutout loop, with rounded control polygons.

See [the hilt construction schema](review/hilt-construction.md) for JSON
authoring, ornament sockets, graph bindings and the bounded cutout contract.
Axe plates thin from their reinforced root toward
the cutting edge, expose independent shoulder cusps, and carry an opposing
fluke/poll when mirrored. Spear points use a diamond section with distal taper.

Center-gripped round shields have a hand aperture beneath their hollow boss;
strapped shields retain a continuous body. The pavise has a readable central
rib and reference-oriented proportions. Preset descriptions distinguish period
contexts, older retained equipment and studies outside the 1544 setting.

## Screenshot review loop

Open **Review gallery** for default and seeded specimens. The repeatable
[capture and independent-review workflow](review/iteration.md) records exact
inputs and supports fixed-fixture replay and adversarial joint/LOD cases.
The **Pommel** and **Guard** focus buttons frame furniture with its immediate
connection context. Capture views such as `front-pommel`, `oblique-pommel`,
`rear-pommel`, and `oblique-guard` use the same semantic bounds at every LOD.
[Artistic criteria and museum references](review/artistic-criteria.md) include
the first iteration’s findings. The [hilt review record](review/hilt-artistic-criteria.md)
records the independent reviewer’s construction and LOD acceptance decision.
