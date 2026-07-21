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
`target/strategic-map/strategic-map-v1.json` presentation package and
`target/strategic-map/strategic-map-tiles-v1.pack` world-tile asset. `just
build-strategic-map` derives both versioned files with an embedded content digest from the
initialized Viabundus v2 roads, ferries, and 1500 water polygons, generalized
Copernicus GLO-30 elevation, and every available prepared Copernicus forest
tile. It clips the view to the supported northern-European envelope and
simplifies source geometry for a multilevel raster presentation; it does not change or replace
canonical routing data. Native elevation is classified into hilly areas and
multi-scale mountain ranges rather than absolute-height colour bands. Forest
coverage is deliberately partial: the generator renders only
installed TCD/DLT tile pairs and records their exact bounds instead of filling
missing regions with inferred vegetation.

The server exposes individual AVIF images from an indexed pack beneath a small
inline SVG settlement overlay. Each tile URL includes the pack's SHA-256 and is
served with `public, max-age=31536000, immutable`. The browser loads only tiles
covering the current viewport at an appropriate zoom from the Paper pyramid.
The offline raster compiler belongs to `adventuresim-world-import`, not the web
server. `strategic-web` optionally loads `STRATEGIC_MAP_BUNDLE_DIR` (default
`target/strategic-map`) at runtime and has no raster renderer or encoder
dependency. A missing or invalid bundle does not prevent startup: settlement
selection and direct travel continue through the surrounding HTML interface.
Current and selected settlements, locally issued available quests, the party's
active quest when it is at the issuing settlement, direct-route state,
population-class settlement pictograms, collision-managed names, destination
links, and the straight selection line remain dynamic HTML/SVG and are served
on every map response. Label priority responds
to the current zoom while the raster package remains unchanged and cacheable.
A four-pixel encoded gutter prevents seams between lossy
tiles, while the deepest level uses high-quality AVIF and native terrain
classification instead of magnifying generalized source cells. The map uses a
single green forest mask at 20 percent canopy, light brown for hilly open
ground, and dark green where forest and hills overlap. Mountain ridge marks
require multi-scale local relief, sustained steepness, and connected area;
absolute elevation alone is insufficient. Forest cell densities form one
continuous field, with boundaries displaced by
deterministic domain-warped, four-octave noise and antialiased from four samples
per output pixel so the prepared low-resolution grid does not appear as square
pixels. The coverage remains offline raster content rather than dynamic browser
geometry. Viabundus zoom importance also
controls road visibility, weight, and ink strength, while a subtle globally
positioned parchment texture keeps adjacent tile gutters identical.

The stable `strategic-map-v1.json` and `strategic-map-tiles-v1.pack` filenames
are versioned, not content-addressed; the pack digest query parameter is every
tile route's cache key.
The compact schema-3 deployment manifest carries renderer revision 5, reviewed
source identities, coverage counts, and the indexed AVIF pyramid, but not the
offline roads, compound water rings, elevation cells/contours, or forest
regions used to render it. Its embedded SHA-256 covers every deployed field;
strategic-web revalidates that digest and the exact source URLs before
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
