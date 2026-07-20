# Viabundus world data

The strategic world-import pipeline uses **Viabundus Pre-modern Street Map 2**,
version 2 (released 25 April 2025), edited by Bart Holterman et al.

- Source record: <https://doi.org/10.5281/zenodo.16611998>
- Project: <https://www.viabundus.eu>
- License: [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/)

The initializer sidecar records the byte size and SHA-256 of each downloaded
CSV. Import requires a bounded, deny-unknown v2 sidecar with the canonical
Zenodo record, unique safe CSV names, and an inventory of every consumed CSV;
it verifies consumed bytes before granting reproducible snapshot status.
Legacy sidecars without sizes remain explicitly release-blocked.

The upstream CSVs are downloaded locally into the Git-ignored `viabundus/`
directory with `just init-viabundus`. The native Rust world compiler reads them
from its source-specific `sources::viabundus` module, then enriches the draft
with required values from the other initialized sources. `just compile-world`
writes the validated, schema-versioned artifact to
`target/world-1544.json`. The generated strategic graph contains
the source attributes required to route between and identify settlements in
1544: nodes, active land/ferry edges, settlement metadata, active alternative
names, and settlement/city descriptions. Description HTML entities are decoded
and source markup is removed by the Viabundus parser, so only plain text enters
the source-independent world schema. Each settlement also retains its
approximate population estimate. It is an adapted
dataset and must retain this attribution and CC BY-SA 4.0 licensing when
distributed.

The settlement Map screen uses the separately generated
`static/map/strategic-map-v1.json` presentation package and
`static/map/strategic-map-world-v1.svg` world-geometry asset. `just
build-strategic-map` derives both versioned files with an embedded content digest from the
initialized Viabundus v2 roads, ferries, and 1500 water polygons, generalized
Copernicus GLO-30 elevation, and every available prepared Copernicus forest
tile. It clips the view to the supported northern-European envelope and
simplifies source geometry for an SVG overview; it does not change or replace
canonical routing data. Elevation is emitted as bounded tint cells and contour
segments. Forest coverage is deliberately partial: the generator renders only
installed TCD/DLT tile pairs and records their exact bounds instead of filling
missing regions with inferred vegetation.

The server renders the world asset as an external SVG layer beneath a small
inline settlement overlay. The external URL includes the SVG's own SHA-256 and is
served with `public, max-age=31536000, immutable`, so roads, water, elevation,
and forests are downloaded once per generated package. Current and selected
settlements, direct-route state, and pin links remain dynamic HTML/SVG and are
served on every map response.

The stable `strategic-map-v1.json` and `strategic-map-world-v1.svg` filenames
are versioned, not content-addressed; the world SVG's digest query parameter is
its cache key.
Its embedded SHA-256 covers schema, year, bounds, all source identities and
statuses, roads, water, elevation cells and contours, forest regions, and
partial-coverage bounds; strategic-web revalidates that digest before
rendering. A
legacy initializer sidecar without recorded byte
sizes is accepted only with the explicit
`legacy-release-blocked-missing-sizes` package status.

Active Viabundus bridge and toll nodes are projected onto their incident travel
edges with their `from`, `to`, or `both` endpoint identity intact. Ferry routes
and land routes with an optional bridge are distinct enum variants, so invalid
combinations cannot enter the import schema. These are edge properties rather
than settlement properties so travel encounters and tactical scene generation
can use them without implying that the infrastructure lies inside a neighboring
settlement. Contradictory equal start/end years are retained in the compiler's
source model and reported, but do not invent an active feature interval.

Each imported settlement has the prototype's shared merchant services, and
newly created characters start at a random loaded settlement.
The settlement overview lists historical aliases and exposes one deterministic
historical description with its source language; population-based English
flavor text remains the primary description. Non-settlement description
categories such as bridges, tolls, and ferries remain deferred and are counted
by category in the compiler build report.

The import does not claim that every represented line is an exact historical
road. Viabundus' `certainty` value is preserved on each travel edge so gameplay
and presentation can account for uncertain reconstructions later.

The source parser and world-building orchestration live in
`adventuresim-world-import`. Source-independent import records live in the
lightweight `adventuresim-world-schema` crate shared with the strategic
SpacetimeDB module. Heavy source readers must remain in the native importer so
they cannot add filesystem or geospatial dependencies to the database module.
