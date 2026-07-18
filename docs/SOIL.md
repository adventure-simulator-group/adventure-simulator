# SoilGrids prediction and soil finalization

Settlement soil is based on [ISRIC SoilGrids](https://www.isric.org/explore/soilgrids),
rolling version 2, under CC BY 4.0. Restricted legacy European vector data is
not an accepted runtime or distributable input.

The compiler consumes a prepared, content-addressed EPSG:3035 subset rather
than making network calls. The fixed contract contains 250 m predictions for
six depths (0–5, 5–15, 15–30, 30–60, 60–100, and 100–200 cm), and Q0.05,
Q0.50, mean, and Q0.95 for sand, silt, clay, coarse fragments, organic carbon,
pH, CEC, and bulk density. SoilGrids publishes the two water-retention products
only as 1 km aggregated means, which are resampled onto the requested canonical
grid without inventing unavailable quantiles. It also
contains the most-probable WRB group and Histosols/Leptosols probabilities.

## Preparing data

The initializer defaults to a plan because European preparation is large and
requires GDAL:

```powershell
python scripts/init_soilgrids.py --grid-cell-size-meters 1000
python scripts/init_soilgrids.py --prepare --grid-cell-size-meters 1000
python scripts/init_soilgrids.py --verify-only --grid-cell-size-meters 1000
```

Only strictly constructed official `files.isric.org` WebDAV/VRT URLs are used.
GDAL opens the exact allowlisted master VRT through `/vsicurl/` so relative tile references
remain attached to the official URL. Only the exact HTTPS host and
`/soilgrids/latest/` path are accepted. Metadata-probe redirects are disabled
and fail closed; a metadata redirect requires an explicit code/source-contract update. Preparation is
fixed to the aligned EPSG:3035 Europe extent and invokes `gdalwarp` with
`-t_srs EPSG:3035 -tr N N -tap`. The atomic manifest records retrieval time,
source inventory, source and prepared sizes/SHA-256 hashes, extent, origin,
CRS, and cell size. A complete generation is staged under a content-addressed
directory and becomes active only when the root manifest pointer is atomically
replaced, so a failed 207-file build cannot corrupt the prior generation. The
Rust importer rechecks inventory, hashes, dimensions, nodata, Float32 band and
compression shape, EPSG:3035 GeoKeys, transform, units, and canonical grid.

`latest` is a mutable rolling publication, not an immutable source pin. The
manifest therefore records `source_reproducibility: unpinned-rolling-latest`,
observational source SHA-256/size, ETag and Last-Modified when supplied, and
exact prepared hashes. Because rolling source bytes can change between the
observation and GDAL requests, those source fields do not bind the prepared
bytes. The prepared SHA-256 is the authoritative local snapshot; future raw
The canonical source manifest uses the strict preparation manifest's actual
`retrieved_at` value and prepared-manifest digest. This identifies the local
snapshot but does not claim that the rolling `latest` source is reacquirable.
reacquisition is not claimed reproducible.

Preparation requires both `gdalwarp` and `gdalinfo`. Every staged TIFF must pass
end-to-end JSON inspection (fixed size/extent, exact transform, EPSG:3035,
single Float32 band, NaN nodata, and DEFLATE) plus a second prepared hash check
before the active manifest is atomically replaced.

The fixed continental extent currently fits the importer's 32-million-pixel
bound only at exactly dividing sizes of 1 km or coarser (for example 1 km and
5 km). Although the global `SpatialGridSpec` still permits 250 m, SoilGrids
250/500 m preparation is rejected pending tiled ingestion; 750 m is rejected
because it does not divide the extent exactly.

The repository does not claim a complete official European audit until those
layers are prepared. Plan and unit-test modes remain useful without GDAL.

## Typestate and rules

1. After Jung PNV and EU-Trees4F, depth values are thickness-weighted into
   0–30 cm summaries; water capacity uses 0–100 cm. Quantiles are aggregated
   separately and checked for ordering. Texture sums, units, nodata, ranges,
   and grid identity are trust boundaries.
2. A private prediction draft retains exhaustive WRB, Histosols/Leptosols
   probabilities, texture, water, carbon, stones, acidity, CEC, fertility,
   confidence, and modeled-or-inferred evidence.
3. Geology consumes that prediction and elevation. Parent material in final
   soil is typed geology lithology, never a free-form soil-source code.
4. Religion and drought run; EU-Hydro returns a resolved hydrology draft.
5. The soil finalizer combines prediction, geology, hydrology, Jung wetland
   evidence, and elevation. Peat requires Histosols probability, wet PNV, and
   hydrology together—organic carbon alone never creates peat. Shallow/rock
   uses Leptosols plus geology/elevation. Drainage/flooding use WRB, retention,
   and hydrology.
6. The final historical-environment stage consumes the finalized profile. It
   can use acidity/fertility for heath, shallow/rocky/dry conditions for sparse
   cover, and convergent soil/hydrology/Jung evidence for wetland. It does not
   mutate peat from SOC alone, invent slope/roughness, or replace geology-derived
   parent material.

Rules are deterministic and versioned. A full prepared-source audit remains
blocked by the absent official HYDE 3.5, forest, EU-Hydro, and prepared SoilGrids
inputs; this repository does not claim full #67/#68 source coverage.
