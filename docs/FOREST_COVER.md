# Forest-cover world data

Settlement-scale forest cover comes from the **Copernicus Land Monitoring
Service High Resolution Layer Tree Cover and Forests**, using the 2018 Tree
Cover Density (TCD) and Dominant Leaf Type (DLT) products. This is modern data,
used as a plausibility input for the game's 1544 setting rather than as a claim
about the exact historical tree cover around a settlement.

- Product family: <https://land.copernicus.eu/en/products/high-resolution-layer-forests-and-tree-cover>
- DLT dataset DOI: <https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a>
- Terms: Copernicus full, free, and open data policy

The Copernicus download requires an authenticated data-service workflow that
was not available while this integration was developed. The raw source
directory is therefore empty, the integration is verified against synthetic
GeoTIFF fixtures, and a full-source build is not claimed. It is not yet part of
the data-initialization script.

## Manual preparation contract

Reproject and aggregate the official 2018 status layers into 1000-by-1000-pixel
one-degree, EPSG:4326, `RasterPixelIsArea`, single-band UInt8 GeoTIFFs. This
fixed 0.001-degree grid is approximately 100 m at European latitudes and makes
the prepared format deterministic. Place the files in the Git-ignored
`target/world-data-sources/raw/forest-cover/` directory. That directory must
also contain `forest-cover-manifest.json` with exactly this version marker:

```json
{"format":"adventuresim-copernicus-forest-2018-v1"}
```

The marker identifies the source year, resolution, aggregation rule, and class
mapping described here so a raw, stale, or differently prepared byte raster is
not silently interpreted under this contract. Each used degree tile requires a
pair named for its southwest corner:

The marker is not a content inventory. Canonical provenance therefore remains
release-blocked/non-reproducible until every consumed TCD/DLT tile has a checked
size and SHA-256.

- `TCD_N48_E002.tif`
- `DLT_N48_E002.tif`

Southern and western coordinates use `S` and `W`. Both rasters in a pair must
be 1000 by 1000, have identical transforms, and span exactly one degree. TCD
values are canopy percentages from 0 through 100. DLT preparation aggregates
the source 10 m broadleaf and conifer pixels to the same approximately 100 m
grid as TCD: emit `1` for at least 75% broadleaf, `2` for at least 75%
coniferous, and `3` for a mixture where neither type reaches 75%. Use `255` for
nodata in either layer. This code `3` is part of the preparation contract; it
is not asserted to be a raw DLT status-layer class.

The importer groups settlements by degree tile and reads only tile pairs that
contain settlements. A source density of zero becomes `ForestCover::Open`.
Positive density becomes `ForestCover::Wooded(Woodland)`, whose bounded
`CanopyDensity` makes zero-density woodland unrepresentable and whose required
`DominantLeafType` is broadleaf, coniferous, or mixed. There is no unknown
variant.

If density is nodata, the importer creates a deterministic plausible density
from HYDE natural/seminatural land use; cells with less than 5% natural land
become open. If only leaf type is missing, elevation supplies a deterministic
broadleaf/mixed/conifer fallback. The build report counts every settlement
where either fallback was used. Malformed GeoTIFF structure, unsupported
raster encodings, mismatched paired transforms, and missing required tiles are
not silently accepted. Reserved or unclassified cell values take the
documented plausible fallback path and are counted.

Forest cover is stored on settlements because it describes the immediate area
and can drive timber and foraging products, scene vegetation density, visibility,
encounters, and fuel availability. Continuous route or regional forest data
belongs in later spatial products rather than being inferred from one
settlement sample.
