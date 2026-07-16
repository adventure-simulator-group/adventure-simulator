# Tree-species world data

Plausible settlement tree species come from **EU-Trees4F v2**, a modeled
dataset for 67 European tree species. The importer uses the current-climate
ensemble layers as environmental evidence for world generation; it does not
treat them as observations from 1544.

- JRC project page: <https://forest.jrc.ec.europa.eu/en/activities/forests-and-climate-change/>
- Dataset and CC0 licence: <https://doi.org/10.6084/m9.figshare.17032328>
- Data descriptor: <https://doi.org/10.1038/s41597-022-01128-5>
- Archive SHA-256: `be115f771e5598e6fd180621e1a32922880cf7ac8e2cb59ba0eabd7f15bfeda4`

Keep `EU-Trees4F_ens-clim.zip` at
`target/world-data-sources/raw/tree-species/`. Override it with
`--tree-species-archive`. The archive remains a manually initialized source
until its game usefulness is accepted.

## Source semantics and parsing

For each species, the importer requires one complete current-climate triplet:

- `prob_pot`: modeled habitat suitability on a 1/12-degree EPSG:4326 grid,
  stored as an integer score from 0 through 1000. This is not abundance or a
  literal occurrence probability.
- `bin_pot`: the suitability surface after the source's species-specific
  threshold, on a 10 km EPSG:3035 grid.
- `bin_nat`: the thresholded surface masked by the source's expert native-range
  evidence, on the same 10 km grid.

The `cur2005` filename label is a reference scenario, not a direct 2005
observation. The model combines occurrences centered roughly around 2005 with
older climate normals that better represent conditions under which established
trees grew. One retained species, `Robinia_pseudoacacia`, is naturalized rather
than historically native to Europe.

The importer reads the DEFLATE-compressed ZIP in memory-bounded chunks without
extracting paths. It requires the pinned 67-species current-layer manifest,
validates signed Int16, nodata, dimensions, PixelIsArea transforms, CRS keys,
units, and layer value domains, and rejects impossible `bin_nat=1/bin_pot=0`
states. Probability and binary rasters deliberately use different grids and
are sampled independently. A binary candidate whose probability cell is
nodata is omitted rather than assigned an invented score.

## Canonical model and use

Each settlement stores `TreeSpeciesProfile`, with no unknown state:

- `Modeled` contains the twelve highest-suitability candidates at most. Every
  candidate has a validated scientific-name identifier, a bounded suitability
  score, and either within-native-range or outside-native-range evidence.
- `Inferred` contains a nonempty, duplicate-free list selected from typed
  potential vegetation when the rasters yield no modeled candidate.

The constructors keep profiles nonempty, unique, bounded, and deterministically
ordered. The same constructors are used by JSON parsing and the SpacetimeDB
import reducer, so invalid states cannot bypass the source boundary.

The official archive was decoded in full: all 201 current rasters passed their
contracts. Sampling all 6,041 active Viabundus settlements produced modeled
profiles for 5,986 and deterministic vegetation-based profiles for the
remaining 55, retaining 71,053 ranked candidates in total.

Game systems can use these profiles for timber and fruit products, fuel and
charcoal availability, forest tactical scenes, shipbuilding or carpentry
materials, forage, and visual biome selection. These are plausibility inputs,
not claims that a named settlement historically grew every listed species.
