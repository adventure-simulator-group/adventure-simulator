# Hilt construction schema

The editor exposes construction choices rather than a single collection of
unrelated shape sliders. `pommel` is the component kind for rotational,
faceted, fluted, plate, outline and composite furniture. The former separate
fan-pommel component is now an outline construction.

## Pommels

`construction` selects one of these forms:

| Value | Geometry and controls |
| --- | --- |
| `lathed` | Authored increasing `[height,radius]` profile, with breadth and length scales. The editor provides bulb, pear and scent-stopper profiles. |
| `plate` | Wheel in the blade plane, with diameter, height, plate thickness, rim bevel and face convexity. An overlapping neck seats the grip above the wheel. |
| `faceted` | Generated bun profile with intentional facet count. LOD does not turn these facets into a cylinder. |
| `writhen` | Generated fig profile, flute count, depth and signed twist. Vertices follow the helix, and flute-aware angular/axial floors preserve it at low detail. |
| `outline` | Fan or fish-tail control outline, thickness, notch and lobe spread. The fish-tail fork faces away from the grip. |
| `composite` | A simple `baseConstruction` plus ornaments attached to named `sockets`. |

The dimensional forms use `diameter`, `height`, `thickness`, `widthScale` and
`lengthScale`. A profile belongs to the lathed construction; other forms derive
their own profile. Round portions share indexed normals, while deliberate
facets and flat faces remain split.

Composite ornaments use a socket name, a style, scale and optional Euler
rotation. The built-in crown and escutcheon are geometry demonstrators. The
`authored` style accepts flat position arrays and triangle indices, allowing
curated sculpted faces, animals, crowns or emblems without forcing anatomy
through procedural sliders. A simple socket definition looks like this:

```json
{
  "construction": "composite",
  "baseConstruction": "faceted",
  "sockets": {
    "distal": [0, 0.004, 0],
    "front": [0, 0.027, 0.01]
  },
  "ornaments": [
    {"style": "escutcheon", "socket": "front", "scale": 0.022, "rotation": [0, 0, 0]}
  ]
}
```

These are fields within a complete pommel component, not a complete weapon
definition. Coordinates and scale use metres. Nonzero socket Z identifies the
front or rear face; an ornament placed inside its base seats outward against
the actual base surface. This keeps relief visible as the base diameter or
construction changes. A zero-Z distal socket remains an explicit position.
Authored ornaments must contain a closed, consistently oriented indexed mesh;
the same validator used by the rest of the model rejects broken geometry.

## Guard members and terminals

`guard` supports `mirrorMode` values `opposed`, `symmetric` and `independent`.
The first two use total `width` and `sweep`. Independent mode uses each arm's
`leftLength`/`rightLength`, `leftSweep`/`rightSweep`, and
`leftSet`/`rightSet`, with set controlling depth out of the blade plane.

Members select `round`, `oval`, `diamond`, `flat` or `triangular` sections,
`sectionWidth`, `sectionDepth` and signed `sectionTwist` in degrees. The section
frame is parallel-transported along the centerline, with authored roll applied
separately. Polygon edges remain hard. A closed member requires whole-turn
roll so its seam is continuous.

The terminal library contains `none`, `ball`, `disk`, `pyramidal`, `scroll`,
`fishtail` and `vase`, controlled by `terminal` and `terminalSize`. Independent
layout exposes separate `leftTerminal` and `rightTerminal` choices; `shared`
uses the common terminal choice. End ornaments follow the
3D tangent of their own quillon.

## Connected compound hilts

A `guardAssembly` declares `nodes`, an `anchorNode`, and `members`. Each member
names an ordered `path` through the nodes and its section properties. Members
interpolate through the shared nodes, so changing a junction moves every
connected branch. All members must belong to one graph connected to the
anchor. The component still attaches through the weapon's normal named-frame
system.

`nodeBindings` keeps selected nodes dependent on external frames or on two
other nodes:

```json
{
  "nodeBindings": {
    "bowLower": {"frame": "grip.base", "offset": [0.008, 0.002, 0]},
    "bowMid": {"between": ["bowLower", "right"], "t": 0.5, "offset": [0.055, 0, 0.012]}
  }
}
```

Frame bindings resolve first. A `between` binding interpolates two direct
nodes; chained interpolation bindings are rejected. The riding-sword bow uses
this relationship so minimum and maximum grip lengths preserve its lower
connection instead of stretching around a fixed offset.

## Shell plates

Optional `plates` use named-node `outline` and `cutout` loops, `thickness`,
`dishDepth` and `rimRadius`. The loops are rounded control polygons, with
matching vertex counts and ordering. This bounded construction supports one
true opening, a dished annular surface and a rolled outer rim. It does not
claim arbitrary multiple-hole Boolean topology. Use connected members or
separate plates for additional openings.

The default c.1540 riding sword keeps its open branches. Its pierced shell
option and the double-shell adverse fixture are explicitly later-style
construction studies, not ordinary 1544 German equipment defaults.

## Review and LOD

The adversarial fixture set covers constructor extremes, composite ornaments,
all section/terminal families, a plainly visible flat-member half-turn, a
curved pierced shell and both grip-length extremes of the bound compound bow.
The existing fixtures remain available for exact-input replay after fixes.

Use `front-pommel`, `oblique-pommel` and `rear-pommel` to inspect ornament and
grip seating. Use `front-guard` and `oblique-guard` to inspect member roll,
plates and graph junctions. Pommel views include immediate grip context;
guard views include the upper grip and lower blade. All capture LODs use the
high-detail semantic bounds to make side-by-side comparisons reproducible.

Writhen vertices follow a continuous analytic fig profile and helical tracks.
Angular density follows flute depth and radial error; axial density follows
surface travel and LOD error. The deep-twist adverse pommel uses 1,248 / 1,456 /
3,360 triangles at low / medium / high detail. The default Zweihänder pommel
uses 1,248 / 1,248 / 3,024. These budgets preserve the spiral without allowing
a small pommel to consume tens of thousands of triangles.

The authored relief example is labeled **Authored lozenge**: it demonstrates
indexed module attachment, rather than a four-lobed ornament.
