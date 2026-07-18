# Soil world data

Settlement soil is derived from [ISRIC SoilGrids 2.0](https://www.isric.org/explore/soilgrids), a modern model baseline under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). It is not a historical observation of 1544: its physical predictions are used as a reproducible baseline, with attribution retained in compiled-world provenance.

## Preparation contract

Run `just init-soilgrids bounds=world-bounds.hamburg-test.json`. The preparer calls SoilGrids' official WCS 2.0.1 endpoint for one exact WGS84 subset per layer, rather than the paused REST API or a project-invented tile scheme. It writes an atomically replaced, git-ignored cache at `target/world-data-sources/raw/soilgrids/`:

- `sand_0-5cm_mean.tif`, `silt_0-5cm_mean.tif`, `clay_0-5cm_mean.tif`
- `soc_0-5cm_mean.tif`, `cfvo_0-5cm_mean.tif`, `bdod_0-5cm_mean.tif`
- `soilgrids-manifest.json`

The strict manifest binds the cache to the source URL, cache format, exact ordered six-layer contract, and canonical `WorldBounds`. Existing outputs are refused unless `--force` is supplied; replacement occurs only after every download and validation succeeds. Do not commit the GeoTIFFs.

The compiler defaults to this cache (`--soil-dir` overrides it) and requires `--world-bounds` so it can reject a cache prepared for different bounds. Every raster must be a matching single-band Int16 EPSG:4326 RasterPixelIsArea GeoTIFF. Missing/malformed manifest or layers are errors. A valid cache pixel with nodata in any required layer produces a full inferred fallback, never a mixed profile.

## Modelled and inferred properties

Valid 0--5 cm source samples are stored as `SoilProfile::Modeled`; these directly determine texture (sand/silt/clay), topsoil organic-carbon class (SOC), stone content (coarse fragments), and deterministic water-capacity class (texture, bulk density, and stones). ISRIC's documented integer conversion factors are applied: texture and coarse fragments divide by ten to percent, SOC divides by ten to g/kg, and bulk density divides by 100 to kg/dm³. Soil depth, water regime, and agricultural limitation remain explicit deterministic inferences from potential vegetation, elevation, and the physical sample. This stage intentionally does not use hydrology, which is enriched later in the pipeline.

`SoilProfile::Inferred` is reserved for source nodata/out-of-coverage and uses only the documented potential-vegetation/elevation fallback. The distinction lets downstream systems and diagnostics avoid presenting a modelled modern baseline as a historical direct observation.
