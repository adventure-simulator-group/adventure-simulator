# Historical land-use world data

The world compiler derives the 1544 land-use profile from **HYDE 3.5 c9**.
HYDE is a global 5-arcminute historical reconstruction: it is regional
evidence, not an exact observation of an individual settlement.

- Project and release archive: <https://landuse.sites.uu.nl/hyde-project/>
- The HYDE 3.5 release README applies [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/)
  to all HYDE data.

## Manual preparation contract

Download the three HYDE 3.5 c9 April 2025 NetCDF inputs from Utrecht
University's public HYDE vault:

<https://geo.public.data.uu.nl/vault-hyde/hyde35_c9_apr2025%5B1749214444%5D/original/gbc2025_7apr_base/NetCDF/>

The directory is protected by an interactive anti-bot page, so the repository
initializer does not automate this download. Use a normal browser and retain
the release filenames. The separate `general_files.zip` release input is still
required for the matching `general_files/garea_cr.asc` cell-area grid.

Place exactly these release files in the Git-ignored
`target/world-data-sources/raw/hyde35-land-use/` directory, or point
`--land-use-dir` at another directory:

- `cropland.nc` / `cropland` — cropland area in km²
- `grazing_land.nc` / `grazing_land` — grazing-land area in km²
- `urban_area.nc` / `urban_area` — urban area in km²
- `general_files.zip` / `general_files/garea_cr.asc` — HYDE grid-cell area in km²

For normal development, `just init-world-data` installs these exact four files
into that directory from the pinned reviewed source-separated input bundle. Its
HYDE component retains a separate notice and exact file inventory; the archive
is not a combined derived world artifact. Manual browser retrieval is the
fallback for preparing or independently auditing the HYDE component. See
`wiki/engineering/world-data/world-data-bundles.md`.

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
The strategic map compiler separately reads raw interpolated HYDE cropland
kmÂ² for every source cell intersecting the playable bounds. It does not reuse
rounded settlement profiles. Largest-remainder rounding gives each HYDE cell a
whole-square quota while preserving the rounded global area with less than
0.5 kmÂ² residual.

Those quotas are allocated to exact EPSG:3035 1 km squares by deterministic
four-neighbour region growth. All settlements seed one global frontier;
settlement-less HYDE cells receive a deterministic fallback seed, and
adjacency crosses HYDE boundaries. Final inferred roads are merged before this
allocation and therefore participate in both suitability and package identity.
Bounded uniform-grid indexes measure exact point-to-line-segment distance
without scanning every feature for every square. Suitability strongly prefers
settlement proximity, gives roads only short-range influence, prefers an access
band near water rather than banks, requires at least 75% of sixteen explicit
within-square samples to be non-water passable land, penalizes 15-degree
slopes/local relief, and weakly penalizes modern canopy. HYDE's observed
historical cropland takes precedence over Jung potential-natural wetland
vegetation because the latter describes the uncultivated counterfactual;
mapped water remains ineligible. A HYDE cell clipped by the playable boundary
may request more area than the arbitrary boundary can
represent with complete, usable canonical squares. Rules version 2
deterministically saturates only those clipped edge cells at their usable-square
capacity, including zero, and reports the omitted area. Un-clipped cells may
saturate only when canonical-grid/raster discretization is at most 2 km² and
at most 5% of the cell quota. Larger coastal or usable-land
contradictions still fail the build. Soil is deliberately omitted:
the current SoilGrids stage exposes settlement samples rather than a bounded
map-wide raster, and expanding that compiler surface is disproportionate to
this allocator.

The selected square IDs, HYDE source digest, rules version, residual, and
counts determine the final terrain package identity. Exact square boundaries
are used for both foraging legality and map paint.
