# Strategic route terrain

World schema v25 and inference rules v9 attach a required `RouteTerrain` to
every imported travel edge. The record is a static strategic fact used for
travel planning and encounter selection. It never persists tactical positions,
damage, HP, enemies, or simulation ticks.

## Geometry and elevation

Viabundus supplies topology and endpoint coordinates rather than complete road
polylines. Documented edges therefore retain the endpoint interpolation below.
Terrain-inferred edges instead sample their canonical A* polyline. The compiler chooses
`N = min(1000, max(1, ceil(length / grid-cell-size)))` segments, yielding a
bounded profile of two through 1,001 samples with unique permille progress and
required 0/1000 endpoints.

Each profile point samples GLO-30 at the center and the eight positions one
configured canonical-grid cell away. The shared strict reader validates tile
georeferences and handles nodata consistently with settlement elevation.
Decoded tiles use a deterministic least-recently-used cache bounded to 64 MiB;
eviction changes memory use only, never sample values or artifact identity.
Missing tiles and terminal voids use the explicit sea-level fallback; the build
report and edge Markdown record that choice.

Consecutive center samples produce ascent, descent, and signed maximum grades.
Interior projected coordinates use sign-symmetric nearest-integer rounding, so
reversing an edge yields the exact reversed coordinate sequence.
The 3x3 neighborhoods produce Horn slope/aspect, mean absolute center-neighbor
difference (TRI), and relief. Aspect is `Flat` below 10 permille mean slope;
otherwise a circular mean selects one of eight closed compass directions.

Rules v6 classify a route as:

- `Flat`: maximum slope below 30 permille and relief below 30 m.
- `Rolling`: below 80 permille and 100 m.
- `Hilly`: below 150 permille and 300 m.
- `Mountainous`: everything else.

A center at least 20 m above/below its eight-neighbor mean marks a ridge or
valley. A likely pass requires opposing high neighbors and lower orthogonal
neighbors. Adjacent identical detections are deduplicated deterministically.

## Water, seasons, and encounters

EU-Hydro crossings and ferry waterways become canonical nearest facts at zero
meters. Feature kinds are river, canal, ditch, inland, tidal, and coastal.
Seasonal rules are deliberately small and exact: ford/ferry facts dominate
spring flood, autumn mud, and winter ice severity; nearby freshwater can add
low mud or ice risk; mountainous or 1,000 m routes add medium winter snow. No
summer-drought route rule exists in v5.

Static encounter tags cover terrain class, steep (at least 150 permille), rough
(TRI at least 20 m), landforms, bridge/ford/ferry, water banks/shores, and each
seasonal hazard. Empty collections mean confirmed absence, never unknown.
Viabundus `slope_multiplier` remains a separate source cost hint and is not DEM
grade.

Both the offline validator and strategic import reducer recompute profile
ascent/descent, grade extrema, relief, class, seasonal risks, and encounter tags.
Malformed or contradictory derived facts are rejected. Collection decoding is
capped before allocation and canonical uniqueness is defined by logical key
(progress, feature, hazard, or tag), not by the complete payload.

The official full GLO-30 and EU-Hydro audit remains blocked until their
authenticated, completely pinned source inventories are available locally.
Synthetic tests exercise deterministic algorithms and strict boundaries; this
document does not claim issue #62 complete.

## Native routing skill mixture

The separate native terrain-routing pack (schema 4) retains canopy and hill coverage
independently. Runtime routing cells normalize them to exactly 1,000 permille:
Forest follows canopy density, Hills receives the hill-covered share of the
remaining non-forest ground, and Plains receives the remainder. Urban is part
of the Terrain skill domain but has zero route weight until an authoritative
built-up-land source is added; roads are infrastructure over their underlying
terrain and are never treated as Urban.

Plains, Forest, Hills, and Urban are intuitive mental checks using Instinct,
Intelligence, focus, and training on the shared rank-zero-to-five curve. Party
checks use bounded party aggregation, then the cell mixture's dot product.
Movement speed is multiplied by `1 + check / 10`. The departure profile is in
the A* cache key and the resulting adjusted spans are validated and persisted,
so skill can affect route choice while an active journey remains stable.

Actual walking trains every living traveler and conserves exposure across the
normalized mixture. Non-road movement grants one exposure hour per movement
hour. Roads still train their underlying terrain, discounted by underlying
off-road speed divided by 5 km/h: 0.25 open, 0.20 sparse woods, and 0.15 deep
woods. Camp intervals grant no Terrain training.

`terrain-routing-base-v2` is a documented-road-only inference input and cannot
be served as the final pack. `terrain-routing-v2` is rebuilt after world
compilation with the accepted inferred polylines. Both manifests carry a purpose,
road-geometry digest, Jung wetland source digest, and package digest. Wetland
ground moves at 0.5 km/h unless overridden by water or a road.
