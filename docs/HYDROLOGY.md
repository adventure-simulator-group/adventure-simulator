# Surface water and road crossings

Settlement water access and travel-edge crossings are sourced from the
**Copernicus EU-Hydro River Network Database v1.3**.

- Product: <https://land.copernicus.eu/en/products/eu-hydro/eu-hydro-river-network-database>
- DOI: <https://doi.org/10.2909/393359a7-7ebd-4a52-80ac-1a18d5f3db9c>
- User guide: <https://land.copernicus.eu/en/technical-library/eu-hydro_user_guide>
- Projection: ETRS89 / LAEA Europe (EPSG:3035).
- Source period: primarily 2006, 2009, and 2012 imagery, supplemented by
  EU-DEM drainage modeling. This is used as plausible geography for 1544, not
  as evidence that every modern canal or watercourse existed then.

Download the basin GeoPackage distribution and extract its `.gpkg` files under
`target/world-data-sources/raw/hydrology/`. Nested basin directories are
accepted. Override the directory with `--hydrology-dir`.

The importer has been exercised against the official Elbe basin GeoPackage as
part of a complete world build. That distribution uses the valid legacy
GeoPackage 1.1 `GP11` application identifier and mixes ISO dimensional type
codes on collection roots with equivalent EWKB Z/M flags on their children.
The source boundary accepts the OGC `GP10`, `GP11`, and current `GPKG`
identifiers and normalizes only those equivalent nested Z/M flags before
decoding. It continues to reject unknown application identifiers, embedded
EWKB SRIDs, incompatible child types or dimensions, truncated geometry,
overflow, empty geometry structures, and trailing bytes. Geometry parsing is
bounded by blob size, nesting depth, structural nodes (including polygon
rings), and coordinate count. X/Y ordinates must be finite and fall in the
deliberately generous -1,000,000 to 10,000,000 metre safety envelope, which
contains the EPSG:3035 European area of use and EU-Hydro source extent. A
feature may occupy at most 10,000 ten-kilometre spatial-index cells. These
limits are cumulative as well: one import accepts at most 256 GeoPackages,
1,000,000 decoded features, 50,000,000 decoded coordinates, and 10,000,000
spatial-index references. Polygon rings must contain at least four coordinates
and be exactly closed in every dimension; lines must contain at least two
coordinates. These defense-in-depth limits keep malformed source data from
causing unbounded parsing or index construction. Read-focused SQLite fixtures
cover those contracts alongside an ignored integration test for a locally
installed official distribution.

`just plan-hydrology` performs a redacted `CLMS_TOKEN_FILE` preflight and
prints the fixed v1.3 request contract. Because official archive/item IDs and
the complete basin GeoPackage inventory are not pinned, `init-hydrology`
refuses network acquisition. `verify-hydrology` validates a supplied strict
local inventory; the source remains release-blocked.

## Parsed source features

The compiler recognizes the official `River_Net_l`, `Canals_l`, `Ditches_l`,
`InlandWater`, `Transit_p`, and `Coastal_p` feature classes. It also accepts
the equivalent names exposed by the EEA map service. Relevant features are
clipped to a ten-kilometer margin around the imported world before enrichment.
When a basin GeoPackage provides the standard RTree extension, the SQL reader
applies that envelope before decoding geometry; packages without it use a
compatible full-table scan and the same exact geometry-bounds filter.

For flowing water, `STRAHLER`, `HYP`, and `NVS` become bounded Strahler order,
perennial/intermittent/ephemeral persistence, and navigability. Dry source
segments are omitted. Missing and sentinel attributes are resolved to
plausible defaults while parsing; raw `-9999`, null, or unknown values never
enter the canonical schema. `AREA_GEO` classifies inland water by gameplay
size, with geometry bounds as a deterministic fallback.

## Canonical settlement model

A settlement independently records nearby flowing, inland, and marine access
within two kilometers. Flowing access is either a river or a river with a
nearby canal, so a canal-only settlement state cannot be represented. Inland
water is fresh pond/lake/great-lake access. Marine water is either tidal
(treated as brackish for gameplay) or open coast (salt water). Absence means
the settlement is landlocked with respect to that category, not that salinity
is unknown.

These distinctions can drive fresh- and salt-water foods, harbor or fishing
work, water transport, irrigation, flood scenes, and local encounter dressing.

## Canonical edge model

Hydrology finalizes the road draft. A land route owns zero or more typed
river, canal, or ditch crossings, each with its position along the edge and a
plausible bridge-or-ford traversal. A ferry instead owns exactly one river,
inland-water, tidal-water, or coastal-water payload. Consequently a ferry with
land crossings, or a land route with a ferry waterway, cannot be represented.

Straight endpoint-to-endpoint geometry is used because Viabundus currently
imports topology rather than complete road polylines. Existing Viabundus
bridge endpoint evidence wins over the size-based bridge/ford inference. If a
known bridge has no mapped EU-Hydro segment, the compiler supplies a plausible
small perennial river crossing at that endpoint. Ferry edges without a nearby
mapped water feature receive a plausible small perennial river rather than an
unknown waterway.

The subsequent route-terrain stage reuses crossings as zero-distance water
adjacencies. The nearest feature of each other category may be retained within
the existing two-kilometer threshold when the full official distribution is
available. These facts feed only versioned static seasonal-risk and encounter
selectors; they do not create tactical simulation state.
