# City walls implementation plan

Status: internal implementation plan for discussion. This is not public wiki
copy and does not establish historical dimensions or art direction.

## Objective

Generate one deterministic fortification circuit as part of the city layout.
The circuit may curve freely through world space and is not aligned to any
building grid. It must nevertheless resolve into simple locally framed spans
that can be meshed, UV mapped, assigned LODs, collided with, and made walkable.

The fortification must participate in city generation rather than being placed
as decoration afterward. It defines gates, constrains streets and lots, shapes
terrain locally, and determines the developed city's outer envelope.

## Core decisions

1. The planning representation is a closed curve, but the built wall is a
   joined chain of short straight masonry spans.
2. Curves are never queried directly by gameplay. A deterministic sampled
   polyline is the authoritative geometric representation used by every
   downstream system.
3. The wall is generated before building lots. Population selects an enclosed
   area with reserve capacity; development then fills inward.
4. Placement does not distinguish the playable area. The same wall exists
   everywhere; only collision and high-detail residency are tactically clipped.
5. LOD selection is based on observer distance, not whether a span lies inside
   the playable area.
6. Close walls are assumed to have a playable wall walk. Major towers and
   gatehouses may later use the high-detail building pipeline.
7. Long uninterrupted wall sections are joined into continuous meshes with
   continuous arc-length UVs. Gates, towers, material changes, and deliberately
   sharp structural turns split batches.

## Authority and data flow

```text
population, seed, terrain summary, historical/civic anchors
    -> WallCircuitPlan
    -> validated sampled WallCircuit
    -> gates, towers, spans, clearance bands, approach-road anchors
    -> streets, blocks, lots, yards
    -> resolved fortification geometry
    -> visual LOD meshes and separate tactical collision
```

The semantic city layout owns the circuit and its features. Render meshes and
colliders are derived products and must not become alternative placement
authorities.

## Proposed semantic model

Names are illustrative; final types should follow the local semantic-type
conventions in `adventuresim-tactical-core`.

```rust
struct CityFortification {
    circuit: WallCircuit,
    spans: Vec<WallSpan>,
    gates: Vec<WallGate>,
    towers: Vec<WallTower>,
    clearance: WallClearanceBand,
}

struct WallCircuitPlan {
    control_vertices: Vec<WallControlVertex>,
    winding: CircuitWinding,
}

struct WallCircuit {
    samples: Vec<WallSample>,
    total_length_metres: f32,
}

struct WallSample {
    position_metres: Vec2,
    tangent: Vec2,
    outward_normal: Vec2,
    distance_along_circuit_metres: f32,
    source_segment: WallControlSegmentId,
    source_parameter: f32,
}

struct WallSpan {
    id: WallSpanId,
    sample_range: Range<usize>,
    construction: WallConstruction,
    height_metres: f32,
    thickness_metres: f32,
    parapet: ParapetKind,
}
```

Gates reference an approach-road anchor and a position along the circuit.
Towers reference a circuit position, footprint class, and rotation derived from
the local tangent. IDs should derive from domain-separated seed values and the
source control segment plus quantized local parameter, rather than mutable
vector indices.

## Circuit generation

### Enclosed area

Estimate required developed area from population and the existing house-class
capacity model, then multiply by a reserve factor for streets, markets, civic
sites, yards, gardens, and undeveloped interior land. Choose a restrained aspect
ratio and orient it from terrain or approach-road constraints.

The first circuit should not shrink-wrap selected houses. It should enclose a
defensible planned envelope, after which lots are selected within it. This also
allows lightly developed areas inside a large wall without special cases.

### Control loop

Create roughly 12--20 control vertices from a small number of broad directional
decisions. Avoid independent high-frequency radial jitter, which would produce
a uniformly wobbly fantasy outline. Gates and important terrain/civic anchors
act as constraints on the loop.

Use a closed centripetal Catmull--Rom curve for the first implementation. It
does not require procedural Bézier handles, behaves well through unevenly
spaced control points, and remains editable. Explicit cubic Bézier segments can
replace it later without changing downstream consumers.

### Deterministic tessellation

Sample the planning curve into a closed polyline using fixed maximum chord
error, maximum span length, and maximum tangent-angle change. Subdivision order
must be deterministic. Store the curve source segment and source parameter on
every sample.

The sampled polyline must be validated for:

- closure and consistent winding;
- minimum edge length;
- no self-intersection;
- bounded turning angle;
- sufficient clearance around required anchors;
- valid inner and outer offsets;
- no gate overlaps or tower-spacing violations.

Invalid candidates should be repaired by a bounded deterministic relaxation
pass. Generation must fail explicitly if the bounded repair cannot produce a
valid circuit; it must not fall back to a second legacy placement path.

## Gates, roads, and internal layout

Choose a small set of main approach directions before resolving the circuit.
Intersect their paths with the sampled wall and split the wall at those arc
positions to create gates. Gate placement is therefore a geometric constraint,
not a decorative mesh placed over an uninterrupted collider.

The long-term generation order is:

1. choose civic/historical nuclei and external approaches;
2. generate and validate the wall circuit;
3. create gates at approach intersections;
4. connect gates to the market and nuclei with major streets;
5. derive local street-orientation fields;
6. generate blocks, frontage parcels, yards, and buildings inside the circuit.

For an incremental migration, the existing street-node network may be retained
temporarily. Promote selected street lines to approach roads, intersect them
with the circuit, and reject blocks/lots outside the inner clearance loop. The
temporary dependency should later be reversed so gates help generate streets.

Replace the current fixed-ellipse `block_is_inside_city` decision with geometry
against the wall's inner loop. Use more than a block-centre test: frontage and
building footprints must remain outside the wall-clearance band. Awkward
residual wedges may remain yards, alleys, or undeveloped ground.

## Offset and join geometry

The centerline is offset inward and outward by half the wall thickness.
Unlimited miter joins are forbidden because sharp turns create spikes and
self-intersections.

Join policy:

- shallow turn: bounded miter;
- moderate turn: bevel join;
- strong structural turn: split spans and place a corner tower or explicit
  angled masonry joint;
- invalid inward offset: reject or relax the planning circuit.

The renderer should not extrude a perfectly smooth ribbon. Masonry is built as
straight local spans sharing boundary vertices. The planning curve organizes
those spans but is not itself the visible wall surface.

## Towers and structural features

Place towers deterministically at:

- both sides of major gates;
- strong direction changes;
- bounded intervals along otherwise long spans;
- exceptional civic or terrain anchors when later supported.

Towers hide hard geometric joins and provide stable mesh/collision boundaries.
The first version may use simple solid round or polygonal towers. Enterable
towers and gatehouses should later be expressed through the high-detail
building programme rather than hollowed ad hoc from an LOD facade.

## Visual geometry and UVs

Each sample provides a local frame:

- tangent along the circuit;
- outward normal across wall thickness;
- world up vertically.

Generate continuous indexed surfaces for wall faces, wall walk, parapets, and
foundations. Adjacent ordinary spans share vertices. The horizontal UV
coordinate is cumulative circuit distance divided by material tile length;
vertical UVs use physical height. UVs must remain continuous across ordinary
span boundaries and intentionally restart only at semantic material breaks.

Close geometry should initially include:

- exterior and interior wall faces;
- walkable top or wall walk;
- solid parapet masses;
- crenellation geometry or close-range cutouts;
- gate and tower attachment boundaries;
- foundation/skirt geometry that hides minor terrain contact errors.

## LOD policy

LOD is observer-distance based across the entire circuit.

### LOD 0

- joined wall faces and walkable wall top;
- close parapet/crenellation geometry;
- high-detail towers and gates where resident;
- continuous physical-scale UVs;
- compatible with tactical collision, although collision is compiled
  separately.

### LOD 1

- simplified span cross-section;
- reduced tower/gate detail;
- crenellations represented by an alpha mask where their screen size supports
  it;
- continuous silhouette and UV distance.

### LOD 2

- coarse uninterrupted wall and tower silhouettes;
- no individual crenellations that would shimmer;
- baked large-scale material character;
- aggressively joined spans and low draw-call count.

The far circuit may eventually have a visibility cutoff, but it should first be
large enough that horizon captures do not expose missing wall sections.

## Collision and playability

Compile collision separately from visual LODs.

- Wall bodies can use joined boxes/prisms or bounded convex compounds per
  semantic span.
- The wall walk needs a continuous walkable surface without cracks at sample
  boundaries.
- Parapets need simple continuous collision, not one collider per crenellation.
- Towers and gatehouses receive separate collision products.
- Gate openings must split wall collision exactly where they split visual
  geometry.
- Dynamic gate doors and portcullises are later physics objects owned by the
  gate feature, not by ordinary wall spans.

Only tactically relevant spans require collision residency. This clipping must
not alter wall placement, visuals, stable IDs, or terrain shaping.

## Terrain treatment

Do not flatten the whole circuit to one elevation. Even the first version
should compute a slowly varying foundation grade along the sampled circuit and
flatten only across a narrow corridor containing wall thickness, wall-walk
support, and immediate access ground.

For each span:

1. sample representative terrain heights;
2. choose a bounded local grade;
3. flatten the cross-section to that grade;
4. blend back to original terrain across an outer falloff;
5. level gate approaches with their roads;
6. extend foundation/skirt geometry below the shaped surface.

Stepped foundations, retaining walls, ditches, berms, and terrain-aware tower
bases are deferred, but the semantic model must leave room for them.

## Determinism and serialization

- Domain-separate random choices for circuit shape, gates, towers, span
  construction, and damage/variation.
- Quantize comparisons used for topology decisions where floating-point ties
  could alter feature counts.
- Keep the clean final schema; do not add a legacy ellipse fallback or dual
  placement fields.
- Scene input should serialize semantic wall features and resolved placement
  data needed by tactical systems, not renderer-owned mesh buffers.
- The same seed and population must produce byte-stable semantic output.

## Validation and tests

### Circuit tests

- deterministic output snapshot for representative seeds/populations;
- closed loop with correct winding;
- no self-intersections;
- bounded edge lengths and tangent changes;
- inner/outer offsets are valid and non-self-intersecting;
- adaptive sampling meets chord-error limits;
- cumulative arc length is monotonic and closes consistently;
- point-in-circuit classification is stable near edges.

### Layout tests

- every gate intersects exactly one approach road;
- major streets reach their assigned gates;
- no building footprint enters the wall-clearance band;
- wall placement is unchanged by tactical playable bounds;
- population scaling increases capacity/enclosed area monotonically within
  intended variance;
- undeveloped internal land remains possible.

### Mesh tests

- ordinary wall runs are watertight and use outward winding;
- adjacent spans share boundary positions without cracks;
- no unintended coplanar overlapping triangles;
- UV distance is continuous across span joins;
- gates create real mesh openings;
- tower joins cover or resolve hard corners;
- LOD triangle counts decrease monotonically;
- every LOD preserves the circuit silhouette within an error bound.

### Collision tests

- continuous walkable top across ordinary joins;
- no wall-body collision across gate openings;
- parapet collision prevents traversal where expected;
- visual/collision bounds agree within tolerance;
- collision compilation is limited by tactical relevance without changing
  semantic placement.

### Visual acceptance

Deterministic captures should include:

- entire-city elevated view;
- outside approach toward a gate;
- oblique view along a long wall run;
- sharp turn with tower join;
- wall-walk view across several span joins;
- LOD 0/1 and 1/2 transition distances;
- distant skyline showing the complete circuit;
- terrain-contact views on rising and falling grades.

An independent reviewer should reject visible seams, smooth-ribbon geometry,
spiked offsets, floating or buried foundations, broken gate openings, UV resets,
crenellation shimmer, LOD popping, and implausibly uniform control-point noise.

## Implementation milestones

### Milestone 1: semantic circuit

- Add wall plan/result types to the city-layout domain.
- Generate a population-scaled closed control loop.
- Deterministically tessellate and validate the sampled circuit.
- Implement inside/outside and clearance-distance queries.
- Add circuit property tests and snapshots.

### Milestone 2: layout integration

- Replace the fixed city ellipse with the wall inner loop.
- Select approach roads and place semantic gates.
- Suppress lots against the wall-clearance band.
- Emit the circuit, gates, spans, and towers in scene input.

### Milestone 3: visual spans and LODs

- Build joined wall-span meshes with continuous physical UVs.
- Add simple tower and gate gap geometry.
- Add distance-based LOD 0/1/2 selection.
- Reuse procedural masonry and crenellation-mask texture recipes.
- Add deterministic city-wall capture fixtures.

### Milestone 4: tactical collision and terrain

- Add the separate wall collision compiler.
- Make the wall walk and parapets continuously collidable.
- Shape a graded terrain corridor under the circuit and gate approaches.
- Validate traversal and gate openings in tactical play.

### Milestone 5: high-detail features

- Route gatehouses and enterable towers through the building programme.
- Add stairs/access from ground to wall walk.
- Add dynamic gate doors and portcullises.
- Add ditches, berms, repairs, construction variation, and other historically
  researched fortification features only after the base circuit is robust.

## Explicitly deferred

- siege damage and destructible wall topology;
- star forts, bastions, and artillery-era earthwork systems;
- multiple nested wall circuits;
- castle curtain walls as a separate enclosure hierarchy;
- wall-mounted buildings and houses using the wall as a party wall;
- accurate stepped foundations on severe terrain;
- dynamic rebuilding or urban expansion beyond an existing circuit.

These should extend the semantic circuit and feature model rather than create
parallel freeform-placement systems.
