# Historical land-use world data

The world compiler derives the 1544 land-use profile from **HYDE 3.5 c9**.
HYDE is a global 5-arcminute historical reconstruction: it is regional
evidence, not an exact observation of an individual settlement.

- Project and release archive: <https://landuse.sites.uu.nl/hyde-project/>
- The HYDE 3.5 release README applies [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/)
  to all HYDE data.

## Manual preparation contract

Place exactly these release files in the Git-ignored
`target/world-data-sources/raw/hyde35-land-use/` directory, or point
`--land-use-dir` at another directory:

- `cropland.nc` / `cropland` — cropland area in km²
- `grazing_land.nc` / `grazing_land` — grazing-land area in km²
- `urban_area.nc` / `urban_area` — urban area in km²
- `general_files.zip` / `general_files/garea_cr.asc` — HYDE grid-cell area in km²

For normal development, a reviewed source-separated input bundle may install
these four files into that same directory. Its HYDE component retains a separate
notice and exact file inventory; the archive is not a combined world-data
release. See `docs/WORLD_DATA_BUNDLES.md`.

The importer requires NetCDF-4 inputs with `time`, `lat`, and `lon` dimensions
in that order, matching 4,320×2,160 global 5-arcminute coordinate grids. It
requires HYDE 3.5's `365_day` time axis and verifies matching axes before
sampling. It streams only the requested settlement cells from each large time
slice rather than retaining the full grids in memory.

HYDE expresses land use as areas. The compiler brackets the requested world
year in HYDE's time axis, linearly interpolates cropland, grazing, and urban
area, and divides each by `garea_cr.asc`. At 1544 this interpolates 44% from
the 1500 snapshot toward 1600. The remaining area is natural/seminatural land.
Small source overlap (up to 5%) is normalized deterministically; greater
overlap, malformed values, partial nodata, or missing source files fail the
build. A cell with no usable complete profile receives the documented
deterministic fallback and is counted in the build report.

Canonical land use is stored as bounded basis-point fractions for cropland,
grazing land, built-up land, and natural/seminatural land. The four fractions
sum to exactly 10,000 and support agriculture, livestock, encounters, and
forest-cover fallbacks.
