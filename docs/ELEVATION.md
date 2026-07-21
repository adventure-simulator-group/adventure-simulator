# Elevation world data

Settlement elevation comes from the **Copernicus DEM GLO-30** digital surface
model. The current source is modern rather than a historical reconstruction;
terrain height is sufficiently stable for the game's plausibility-oriented
1544 world generation.

- Product DOI: <https://doi.org/10.5270/ESA-c5d3d65>
- Product information: <https://dataspace.copernicus.eu/explore-data/data-collections/copernicus-contributing-missions/collections-description/COP-DEM>
- Terms: Copernicus DEM licence

The `*_DEM.tif` tiles belong in the Git-ignored
`target/world-data-sources/raw/elevation/` directory. `just plan-glo30` prints
the deterministic request and redacted `CDSE_TOKEN_FILE` preflight,
`init-glo30` refuses until the complete tile inventory is pinned, and
`verify-glo30` checks a strict local `source-inventory.json`. This
release-blocked workflow never logs or stores a token.

`sources::elevation` uses the pure-Rust `tiff` crate and does not require GDAL.
Its strict tile, georeference, nodata, and nearest-valid-pixel reader is shared
with route-terrain enrichment. Route sampling uses a deterministic 64 MiB LRU
decoded-tile cache, while the settlement batch remains grouped one tile at a
time. It groups settlements by one-degree GLO-30 tile, decodes only the 159
tiles used by the current Viabundus settlement set, and releases each raster
before reading the next. A settlement receives a required `ElevationMeters`
value; `ElevationBand` is derived from it rather than redundantly stored, so the
two can never contradict each other. There is no unknown variant. Invalid or
void source pixels are replaced by the nearest plausible pixel within eight
raster cells of the same source tile, then by sea level if that local window is
entirely void. The
build report counts these fallbacks. The verified 1544 build sampled all 6,041
settlements without using a fallback.

The settlement Map presentation classifies a native cell as hilly when its
elevation difference to any of its eight neighbours implies a slope steeper
than 15 degrees using the latitude-correct source-cell distance. Hilly open
ground is light brown; hilly ground with at least 20 percent canopy is dark
green. Low zooms use a naturalized aggregate of the same classifications.

Mountains are deliberately not an elevation band. A one-kilometre summary
grid median-filters native samples, then requires at least 300 metres of local
relief in a seven-kilometre radius together with either 150 metres of local
one-kilometre relief or at least 15 percent steep terrain. Connected components
smaller than roughly ten square kilometres are removed before engraved ridge
marks are emitted. This excludes high plains and isolated DEM spikes while
retaining low but rugged ranges. Browsers receive only compressed tiles; raw
DEM pixels are not served and the presentation layer is not persisted in
SpacetimeDB.

Elevation is stored on settlements because it describes the settlement's own
location and can directly influence scene selection, climate inference,
agriculture, travel preparation, and UI presentation. A future source may add
route elevation profiles. See [ROUTE_TERRAIN.md](ROUTE_TERRAIN.md); settlement
elevation is never used as a proxy for terrain along an entire road edge.

## Native strategic terrain pack

`build-strategic-map` compiles the initialized 5–16°E, 50–56°N source tiles
into `terrain-routing-v1.json` and `terrain-routing-v1.pack`. The pack preserves
each source tile's native 1,800/2,400/3,600 by 3,600 grid instead of expanding
it into database rows or `ElevationCell` structs. Independently deflated
256×256 chunks carry signed elevation, road/open/sparse-woods/deep-woods/water
surface, exact bounded canopy percentage, a native 15-degree hill bit, and an
explicit infrastructure-crossing bit.

The manifest and pack are separately SHA-256 addressed. Readers reject wrong
dimensions, overlapping or truncated chunks, digest mismatches, excess
entries, and oversized decompression. Runtime decompression uses a
deterministic 32 MiB LRU. The compressed pack remains range-readable on disk,
is size-checked before opening, and is stream-hashed at startup, so the complete
native grid never resides in RAM. Chunk I/O and decompression occur outside the
cache lock before a race-safe insert.
Northern Germany's z7 paper tiles sample this pack. The rest of the world also
has generalized z7 tiles, so zooming never produces blank uncovered cells.

The same pack is the strategic pathfinding input. A bounded search window begins
at the native nominal 30 m spacing and coarsens deterministically only when the
hard 750,000-node cap requires it. Eight-neighbour A* minimizes directional
travel time: roads are fastest, open ground is slower, sparse and deep woods
are progressively slower, and positive elevation gain adds an uphill cost.
Water is impassable except where imported infrastructure marks a crossing.
Search costs use seconds internally so rounding does not compound at every
30 m cell; persisted journey time is rounded once to whole strategic minutes.

The initialized land-focused GLO-30 request omits five reviewed, all-water
North Sea cells (54–55°N at 5–7°E). The offline pack compiler represents only
that explicit allowlist at the native 2,400×3,600 latitude-band geometry as
zero-elevation impassable water. Any other missing source tile remains a hard
build error; the exception cannot hide a land-coverage gap.

Routes are simplified to at most 512 geographic points for transport and are
stored only for an active journey together with exact ordered terrain-time
spans and the terrain package digest. The raster remains an optional on-disk
asset with a 32 MiB decompression cache. If it is absent, corrupt, outside its
coverage, or cannot produce a bounded route, the existing HTML travel flow
remains available and labels its straight-line value as a legacy estimate.
