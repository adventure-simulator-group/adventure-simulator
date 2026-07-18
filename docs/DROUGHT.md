# Historical summer drought and wetness

Settlement hydroclimate is sourced from the **NOAA Old World Drought Atlas
(OWDA) v1.0**, a tree-ring reconstruction of annual summer Palmer Drought
Severity Index (PDSI) across Europe and the Mediterranean.

- NOAA study: <https://www.ncei.noaa.gov/access/paleo-search/study/19419>
- Publication: Cook et al. (2015), *Old World megadroughts and pluvials during
  the Common Era*, DOI `10.1126/sciadv.1500561`.
- Grid: 0.5 degree point grid, 114 longitudes by 88 latitudes.
- Time coverage: AD 0 through 2012.
- Compiler input: NetCDF-4 classic-model/HDF5.

Place `owda.nc` at `target/world-data-sources/raw/climate/owda.nc`, or override
the path with `--drought-netcdf`. This remains a manually prepared input until
the integration is accepted. The importer uses a pure-Rust, read-only NetCDF-4
decoder; no NetCDF/HDF5 native library enters the shared schema or database
module.

## Source boundary

The parser requires the documented `lon=114`, `lat=88`, and `time=2013`
dimensions; monotonically spaced longitude, latitude, and annual time axes;
and an f64 `pdsi(lon, lat, time)` variable with the source long name and units.
It reads the twenty summers ending in the selected world year. A cell must have
all twenty finite, three-decimal values or twenty NaNs denoting no source
coverage. Partial histories, infinities, unexpected precision, changed axis
orientation, and values outside the audited PDSI range fail at the source
boundary.

The downloaded 228 MB file was verified directly. Across the complete dataset,
8,826,343 of 20,194,416 values are finite; values span -11.996 to 13.431 and
have exactly three decimal places. The 1525–1544 window contains 5,414 complete
grid cells with stable coverage.

## Canonical model and sampling

PDSI is stored as a bounded signed integer in thousandths, not a raw float.
Standard thresholds derive extreme, severe, moderate, or mild drought; normal
conditions; and mild through extreme wetness. Each settlement stores:

- its reconstructed 1544 summer PDSI;
- mean PDSI over 1525–1544;
- the number of moderate-or-worse drought summers (PDSI at most -2);
- the number of moderately-or-more wet summers (PDSI at least 2).

The history constructor guarantees that both counts fit the twenty-year window,
cannot sum past it, include the current summer in the correct category, and
admit the stored mean under the bounded drought, normal, and wet value ranges.
Canonical data distinguishes reconstructed profiles from complete inferred
profiles; there is no unknown state.

Sampling first uses the containing half-degree grid point. For a coastal or
otherwise missing cell, it selects the physically nearest reconstructed point,
with longitude distance scaled by latitude, up to 1.5 degrees. A location
beyond that limit receives a neutral inferred profile. An audit of all 6,041
Viabundus settlements active in 1544 produced 5,458 direct samples, 583 nearest
neighbors, and no fallbacks.

The data can directly affect current harvests, water stress, fire risk, forage,
and river conditions. The twenty-year history can inform reserves, prices,
migration pressure, and whether a single dry or wet summer is locally unusual.
