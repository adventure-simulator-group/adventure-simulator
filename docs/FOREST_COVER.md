# Forest-cover world data

Settlement-scale forest cover comes from the **Copernicus Land Monitoring
Service High Resolution Layer Tree Cover and Forests**, using the 2018 Tree
Cover Density (TCD) and Dominant Leaf Type (DLT) products. This is modern data,
used as a plausibility input for the game's 1544 setting rather than as a claim
about the exact historical tree cover around a settlement.

- Product family: <https://land.copernicus.eu/en/products/high-resolution-layer-forests-and-tree-cover>
- DLT dataset DOI: <https://doi.org/10.2909/82f93572-9888-47ef-97a1-5cac5985a26a>
- Terms: Copernicus full, free, and open data policy

Initialize the default northern-Germany coverage with:

```bash
just plan-forest-cover
just init-forest-cover
just verify-forest-cover
```

`scripts/init_forest_cover.py` reads `COPERNICUS_CLIENT_ID` and
`COPERNICUS_CLIENT_SECRET` from the environment or the Git-ignored repository
`.env`, without displaying either value. These are Sentinel Hub OAuth client
credentials. They do not authorize direct CDSE OData or S3 object downloads;
the initializer instead uses the official Sentinel Hub Process API and its
public CLMS BYOC collections.

The default integer EPSG:4326 bounds are 5-16 degrees east and 50-56 degrees
north: 66 one-degree tiles covering Germany and useful adjoining parts of the
Netherlands, Denmark, Poland, Czechia, and the Baltic coast. Override them with
`--west`, `--south`, `--east`, and `--north`. Each request is independently
restartable in a staging directory. The existing source directory is replaced
only after all 132 output rasters verify; the previous directory is retained
below `target/world-data-backups/`.

The prepared source inventory records the exact byte size and SHA-256 of every
consumed TCD/DLT tile. A source-separated world-data bundle therefore pins the
installed result even though a later invocation of the upstream processing
service could produce revised bytes. The legacy release verifier still reports
this source as release-blocked because it cannot independently pin the
upstream Process API result before acquisition; the local prepared result is
nevertheless exact and repeatably verifiable after download.

## Prepared tile contract

The initializer requests the official 2018 100 m Tree Cover Density collection
(`edd3c5f5-da8e-463f-8c9a-712aa451d37e`) directly. It derives leaf type from
the official 100 m Broadleaved Cover Density
(`a06a42ae-f899-4a07-a5cd-fb7fd920d6c1`) and Coniferous Cover Density
(`a0edd575-c763-4c4a-a910-631df3df4506`) collections. Those density products
are themselves the official aggregation of the 10 m DLT pixels. A cell is
broadleaf or conifer when that type is at least 75% of its classified tree
pixels, mixed otherwise, and `255` when no leaf type applies.

The resulting rasters are 1000-by-1000-pixel
one-degree, EPSG:4326, `RasterPixelIsArea`, single-band UInt8 GeoTIFFs. This
fixed 0.001-degree grid is approximately 100 m at European latitudes and makes
the prepared format deterministic. They live in the Git-ignored
`target/world-data-sources/raw/forest-cover/` directory. That directory must
also contain `forest-cover-manifest.json` with exactly this version marker:

```json
{"format":"adventuresim-copernicus-forest-2018-v1"}
```

The marker identifies the source year, resolution, aggregation rule, and class
mapping described here so a raw, stale, or differently prepared byte raster is
not silently interpreted under this contract. Each used degree tile requires a
pair named for its southwest corner:

The marker is not a content inventory; `source-inventory.json` separately pins
every prepared TCD/DLT tile by checked size and SHA-256.

- `TCD_N48_E002.tif`
- `DLT_N48_E002.tif`

Southern and western coordinates use `S` and `W`. Both rasters in a pair must
be 1000 by 1000, have identical transforms, and span exactly one degree. TCD
values are canopy percentages from 0 through 100. DLT emits `1` for at least
75% broadleaf, `2` for at least 75% coniferous, and `3` for a mixture where
neither type reaches 75%. Use `255` where no leaf type applies or either source
is nodata. This code `3` is part of the preparation contract; it is derived from
the two official density products rather than asserted to be a raw DLT class.

The importer groups settlements by degree tile and reads only tile pairs that
contain settlements. A source density of zero becomes `ForestCover::Open`.
Positive density becomes `ForestCover::Wooded(Woodland)`, whose bounded
`CanopyDensity` makes zero-density woodland unrepresentable and whose required
`DominantLeafType` is broadleaf, coniferous, or mixed. There is no unknown
variant.

If density is nodata, the importer creates a deterministic plausible density
from HYDE 3.5 natural/seminatural land use; cells with less than 5% natural land
become open. If only leaf type is missing, elevation supplies a deterministic
broadleaf/mixed/conifer fallback. The build report counts every settlement
where either fallback was used. Malformed GeoTIFF structure, unsupported
raster encodings, mismatched paired transforms, and missing required tiles are
not silently accepted. Reserved or unclassified cell values take the
documented plausible fallback path and are counted.

The settlement Map presentation may additionally generalize any installed
TCD/DLT tile pairs into one naturalized forest mask at 20 percent canopy cover.
It retains the exact bounded percentage in its offline inputs rather than
turning presentation data into sparse/deep classes. Hilly forest is rendered
dark green while flat forest is green. This is explicitly partial-coverage
presentation data: absent tiles stay absent, tile coverage is recorded in the
map package, and no missing regional forest is inferred from a settlement
sample.

Forest cover is stored on settlements because it describes the immediate area
and can drive timber and foraging products, scene vegetation density, visibility,
encounters, and fuel availability. Continuous route or canonical regional
forest data still belongs in later spatial products rather than being inferred
from one settlement sample; the generalized raster layer is not such a
canonical world product.
