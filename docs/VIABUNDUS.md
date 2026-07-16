# Viabundus world data

The strategic world-import pipeline uses **Viabundus Pre-modern Street Map 2**,
version 2 (released 25 April 2025), edited by Bart Holterman et al.

- Source record: <https://doi.org/10.5281/zenodo.16611998>
- Project: <https://www.viabundus.eu>
- License: [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/)

The upstream CSVs are downloaded locally into the Git-ignored `viabundus/`
directory with `just init-viabundus`. The native Rust world compiler reads them
from its source-specific `sources::viabundus` module. `just compile-world`
writes the validated, schema-versioned artifact to
`target/world-1544.json`. The generated strategic graph contains
only the source attributes required to route between settlements in 1544:
nodes, active land/ferry edges, and settlement metadata, including each
settlement's approximate population estimate. It is an adapted
dataset and must retain this attribution and CC BY-SA 4.0 licensing when
distributed.

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

The import does not claim that every represented line is an exact historical
road. Viabundus' `certainty` value is preserved on each travel edge so gameplay
and presentation can account for uncertain reconstructions later.

The source parser and world-building orchestration live in
`adventuresim-world-import`. Source-independent import records live in the
lightweight `adventuresim-world-schema` crate shared with the strategic
SpacetimeDB module. Heavy source readers must remain in the native importer so
they cannot add filesystem or geospatial dependencies to the database module.
