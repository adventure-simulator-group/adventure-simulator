# Strategic route terrain

World schema v19 and inference rules v5 attach a required `RouteTerrain` to
every imported travel edge. The record is a static strategic fact used for
travel planning and encounter selection. It never persists tactical positions,
damage, HP, enemies, or simulation ticks.

## Geometry and elevation

Viabundus currently supplies topology and endpoint coordinates rather than
road polylines. The compiler projects endpoints to EPSG:3035 and interpolates
the documented straight segment. It chooses
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

Rules v5 classify a route as:

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
