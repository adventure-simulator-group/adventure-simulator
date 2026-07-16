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

The official full archive is not currently present in the development data
directory, so only the strict source boundary has been verified using
read-focused SQLite fixtures with GeoPackage core metadata, a real EPSG:3035
definition, synthetic geometries, and manually populated RTree tables. The
fixtures exercise the reader but are not a writable GeoPackage conformance
suite. Do not describe a full-world hydrology audit as complete until the
official archive has been run.

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
