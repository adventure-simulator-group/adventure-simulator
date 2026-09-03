# Parametric weapon modeler

This standalone browser tool experiments with modular, parameterized melee
weapon geometry. It is deliberately outside the Rust workspace and does not
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
points, smooth swept bows, side and finger rings, fan pommels, flanged mace
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
Rotational furniture offers authored, bulb, pear and scent-stopper profiles,
with separate breadth and length controls. Crossguards expose tip taper,
terminal swelling and symmetric/opposed sweep. Flattened fan caps have rounded
perimeters and beveled faces. Axe plates thin from their reinforced root toward
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
[Artistic criteria and museum references](review/artistic-criteria.md) include
the independent reviewer�s findings and acceptance decision for this iteration.
