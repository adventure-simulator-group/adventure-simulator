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

The settlement Map presentation separately samples the installed GLO-30 tiles
on a coarse deterministic grid. It emits generalized elevation tint cells and
contour segments into the versioned SVG map package; raw raster pixels are not
served to browsers and the presentation layer is not persisted in
SpacetimeDB.

Elevation is stored on settlements because it describes the settlement's own
location and can directly influence scene selection, climate inference,
agriculture, travel preparation, and UI presentation. A future source may add
route elevation profiles. See [ROUTE_TERRAIN.md](ROUTE_TERRAIN.md); settlement
elevation is never used as a proxy for terrain along an entire road edge.
