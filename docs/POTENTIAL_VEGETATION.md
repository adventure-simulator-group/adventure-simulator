# Potential-natural-vegetation world data

Potential vegetation is sourced from Martin Jung/IIASA, **Current and future
European potential vegetation types v1.1**, DOI
[`10.5281/zenodo.14627466`](https://doi.org/10.5281/zenodo.14627466). The pinned
version was published 2025-01-10 under CC BY 4.0 and covers continental Europe,
including Turkey, at nominal 1 km grain.
Canonical temporal metadata records the model-input window, 1990–2020, rather
than presenting the publication year as a single observation year.

Run `just init-jung-pnv` to download the categorical current raster plus the six
current-class COGs into `target/world-data-sources/raw/jung-pnv/`. The atomic
initializer pins every official filename, byte size, published MD5, and verified
SHA-256, rejects oversized streams, and writes a deterministic adjacent
`jung-pnv-manifest.json`. Rasters and the manifest are ignored local source data;
they are never distributed with the repository. Use `--potential-vegetation-dir`
to select another verified initialization directory.

The importer validates the actual source contract: 5,583 by 4,474 Float32 cells,
1,000 m pixels, top-left `(944000, 5416000)`, 512-square Deflate tiles, NaN
nodata, and the source's user-defined GeoTIFF keys carrying the EPSG:3035 LAEA
parameters. Posterior COGs contain seven interleaved samples named
`mean/sd/q05/q50/q95/mode/cv`; world compilation consumes the mean and validates
the complete band contract. The categorical raster accepts only 1 through 6.
Every import revalidates the manifest's record/version/license/file identities,
the exact byte sizes, and SHA-256 values before opening a raster. COG tiles are
decoded on demand with checked channel lengths and a byte-accounted cache capped
at 64 MiB.

Canonical cells receive nodata-aware area-weighted posterior means. When every
class has a valid posterior, six independently quantized 0..10,000 basis-point
scores are stored; they are not asserted to sum to 10,000. Otherwise the
categorical raster is used, choosing greatest valid overlap with stable class
ties. A cell with neither form of source evidence receives a deterministic
non-unknown class inferred from already typed forest, elevation, latitude, and
HYDE 3.5 context. Reports reconcile posterior, categorical, and inferred outcomes
exactly to settlement count.

Inference-rules version 4 and world schema version 17 identify the complete
post-hydrology synthesis contract. Older artifacts or caches cannot share
identity with Jung-derived or reconstructed historical results.

Potential vegetation remains stored unchanged as the modern-climate ecological
envelope. After soil and hydrology finalization, the compiler separately stores
dominant 1544 cover. The greatest sampled HYDE 3.5 human fraction is selected first
(stable tie order: built, cropland, pasture), then becomes direct only when it
meets its own threshold: 10% built or 35% cropland/pasture.
Deterministic missing-HYDE profiles are never labeled direct. Natural cover is
derived from Jung, Copernicus forest structure,
EU-Trees4F candidates, soil/geology, elevation, hydrology, latitude, and OWDA
moisture. Only genuinely close natural scores use coordinate-and-schema hashing
as a deterministic tie-break. Fallback Jung wetland/marine classes cannot emit
water cover: wetlands require wet soil plus freshwater/tidal convergence, while
transitional water requires tidal evidence.

Attribution/modification notice: Adventure Simulator downloads Jung's published
v1.1 rasters unchanged, then projects settlement cells, area-aggregates posterior
means, quantizes values, and applies documented categorical/inference fallbacks.
