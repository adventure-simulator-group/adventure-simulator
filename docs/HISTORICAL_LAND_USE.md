# Historical land-use world data

The world compiler has a source boundary for **HYDE 3.2** (History Database of
the Global Environment), using the 1500 and 1600 CE slices around the game's
1544 date.

- Dataset record: <https://doi.org/10.17026/dans-znk-cfy3>
- Method paper: <https://doi.org/10.5194/essd-9-927-2017>
- Terms recorded by the repository: CC0 1.0

The official archive is currently protected/restricted and the attempted
download produced no source files. Consequently this integration is verified
against synthetic ESRI ASCII fixtures, not the full HYDE distribution, and it
is not yet part of a data-initialization script.

## Manual preparation contract

Place these upstream rasters in the Git-ignored
`target/world-data-sources/raw/historical-land-use/` directory:

- `garea_cr.asc`
- `cropland1500AD.asc` and `cropland1600AD.asc`
- `grazing1500AD.asc` and `grazing1600AD.asc`
- `urban1500AD.asc` and `urban1600AD.asc`

The reader requires the standard six-line ESRI ASCII header with corner
coordinates and samples every Viabundus settlement directly from each grid.
HYDE values are treated as square kilometres and divided by `garea_cr.asc`.
The 1500 and 1600 values are linearly interpolated to 1544.

Canonical land use is stored as bounded basis-point fractions for cropland,
grazing land, built-up land, and natural/seminatural land. The four fractions
must sum to exactly 10,000, making incomplete and overfull profiles
unrepresentable. Overlapping source areas are proportionally normalized. A
nodata source cell receives a deterministic plausible profile based on the
settlement's stable source ID and population level; the build report counts
every such fallback. Human-use intensity is derived from the profile rather
than stored redundantly.

The profile supports agricultural production, grazing/livestock products,
cultivated-versus-wild encounters, and later adjustment of modern forest
cover. It does not claim that the reconstructed grid is exact at village scale.
