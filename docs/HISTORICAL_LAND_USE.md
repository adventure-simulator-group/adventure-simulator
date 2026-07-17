# Historical land-use world data

The world compiler has a source boundary for corrected **HYDE 3.2.1** (History Database of
the Global Environment), using the 1500 and 1600 CE slices around the game's
1544 date.

- Dataset record: <https://doi.org/10.17026/dans-25g-gez3>
- Method paper: <https://doi.org/10.5194/essd-9-927-2017>
- Conservative operational terms: CC BY 3.0 because the DANS record's CC0
  signal conflicts with the bundled attribution-oriented README. This records
  the stricter treatment and does not claim the conflict is legally resolved.

The official record exposes large archives but the seven consumed files do not
have a committed exact size/SHA-256 inventory, and its CC0 and bundled
attribution signals conflict. Consequently `just plan-hyde` is deterministic,
`init-hyde` refuses acquisition, and `verify-hyde` only checks a supplied strict
local inventory. The workflow does not resolve the rights conflict or claim a
full-source audit.

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
unrepresentable. Finite source overlap of at most 5% is proportionally
normalized and counted; larger overfill is rejected as malformed data. A
nodata source cell receives a deterministic plausible profile based on the
settlement's stable source ID and population level; the build report counts
every such fallback. Human-use intensity is derived from the profile rather
than stored redundantly.

The profile supports agricultural production, grazing/livestock products,
cultivated-versus-wild encounters, and later adjustment of modern forest
cover. It does not claim that the reconstructed grid is exact at village scale.
