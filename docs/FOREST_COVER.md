# Forest-cover world data

Settlement-scale forest cover comes from the **Copernicus Land Monitoring
Service High Resolution Layer Tree Cover and Forests**, using the 2018 100 m
Tree Cover Density (TCD), Broadleaved Cover Density (BCD), and Coniferous Cover
Density (CCD) products. This is modern data,
used as a plausibility input for the game's 1544 setting rather than as a claim
about the exact historical tree cover around a settlement.

- Product family: <https://land.copernicus.eu/en/products/high-resolution-layer-forests-and-tree-cover>
- DLT dataset DOI: <https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a>
- Terms: Copernicus full, free, and open data policy

The source is downloaded through Copernicus Data Space Ecosystem (CDSE), using
the local, Git-ignored `.env` variables `COPERNICUS_CLIENT_ID` and
`COPERNICUS_CLIENT_SECRET`. The preparer reads them only to obtain a short-lived
OAuth token; it does not print, persist, or add them to the compiled world.

## Manual preparation contract

Run the bounded preparer from the repository root:

```powershell
just init-forest-cover bounds=world-bounds.hamburg-test.json
```

It obtains the official 2018 100 m source grids through CDSE Process API and
emits 1000-by-1000-pixel, one-degree, EPSG:4326, `RasterPixelIsArea`,
single-band UInt8 GeoTIFFs. CDSE selects the nearest official 100 m source cell
for each 0.001-degree output cell; it does not re-aggregate the source. This
fixed grid is approximately 100 m at European latitudes and makes the prepared
format deterministic. The output is Git-ignored under
`target/world-data-sources/raw/forest-cover/`. That directory also contains
`forest-cover-manifest.json` with exactly this version marker:

```json
{"format":"adventuresim-copernicus-forest-2018-v2"}
```

The marker identifies the source year, resolution, sampling rule, and class
mapping described here so a raw, stale, or differently prepared byte raster is
not silently interpreted under this contract. Each degree tile intersecting the
configured world bounds requires a
pair named for its southwest corner:

- `TCD_N48_E002.tif`
- `DLT_N48_E002.tif`

Southern and western coordinates use `S` and `W`. Both rasters in a pair must
be 1000 by 1000, have identical transforms, and span exactly one degree. TCD
values are canopy percentages from 0 through 100. DLT is deterministically
derived from the official 100 m BCD and CCD percentages: emit `1` when
`BCD / (BCD + CCD)` is at least 75%, `2` when the analogous coniferous share is
at least 75%, and `3` otherwise. Use `255` where TCD, BCD, or CCD is nodata, or
where BCD and CCD are both zero. This code `3` is part of the preparation
contract; it is not asserted to be a raw DLT status-layer class.

The importer groups settlements by degree tile and reads only tile pairs that
contain settlements. A source density of zero becomes `ForestCover::Open`.
Positive density becomes `ForestCover::Wooded(Woodland)`, whose bounded
`CanopyDensity` makes zero-density woodland unrepresentable and whose required
`DominantLeafType` is broadleaf, coniferous, or mixed. There is no unknown
variant.

If density is nodata, the importer creates a deterministic plausible density
from LUH1 natural/seminatural land use; cells with less than 5% natural land
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
