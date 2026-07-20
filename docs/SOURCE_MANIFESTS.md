# World source manifests

World schema v18 replaces the old name/URL/license labels with a canonical,
typed manifest for every distribution used by the offline compiler. Each entry
has a stable ID, release status, canonical URL and optional DOI, typed licence,
operational notices, access method, spatial and temporal coverage, preparation
recipe, and content identity. Human-readable Markdown remains alongside these
fields for the in-game audit trail.

The compiler sorts entries by stable ID and rejects duplicates, non-canonical
order, empty or oversized fields, missing notices, malformed hashes, report
claims without their source entry, and unknown serde fields. There is no
`Unknown` identity. A source is either reproducible from a raw/prepared SHA-256
or curated revision digest, or explicitly records an unpinned or
release-blocked state. The latter states never report themselves as
reproducible.

## Identity and cache boundary

The manifest digest is BLAKE3 over the world schema version, inference-rules
version, world year, complete `SpatialGridSpec`, and sorted distribution
manifests (including notices and preparation). Changing any field changes the
digest. Source ordering, local paths, and file timestamps do not. The complete
compiled artifact is still hashed separately; both IDs are retained by the
SpacetimeDB import session. No separate world-data cache currently exists, so
this digest defines the key that a future cache must use.

The import session also retains complete deterministic audit Markdown for every
used distribution: ID, name, release, licence, content status and identity,
URL, DOI, notices, access, spatial and temporal coverage, preparation, and
notes. Each typed source is embedded as canonical compact JSON. The compiler
rejects the import before calling a reducer if the fully JSON-quoted argument
would exceed the 24,000-character Windows transport budget; legal text is never
silently truncated.

The stored digest is compiler/operator-attested under the trusted first-caller
import model. SpacetimeDB syntax-checks the digest and binds it to the import
session, but it does not receive the typed manifest and therefore cannot
recompute the digest. It is an audit/cache identity, not independent proof
against a malicious or noncanonical client.

## Operational notices and unresolved boundaries

- **Viabundus:** retain attribution and CC BY-SA 4.0. The project applies a
  conservative BY-SA treatment; compatibility with differently licensed
  combined output remains unresolved.
- **Copernicus DEM:** retain the prescribed Copernicus/WorldDEM production
  credit and European Commission/ESA no-liability notice.
- **HYDE 3.5:** retain HYDE attribution, the CC BY 3.0 licence link, and an
  indication that Adventure Simulator interpolates and classifies the source.
- **Copernicus forest and EU-Hydro:** credit the European Union/Copernicus,
  identify project modifications, and do not imply endorsement.
- **Jung PNV, SoilGrids, and EGDI:** retain CC BY 4.0 attribution and identify
  gameplay conversion changes. EGDI additionally retains the Maltese
  contribution and disclaimer.
- **EU-Trees4F:** retain dataset and publication citation despite CC0.
- **IEG religion:** source images remain rights-reserved and are not
  redistributed. Only the coarse curated intermediate is checked in.
- **NOAA OWDA:** cite NCEI and Cook et al.; compiled output remains bounded
  per-settlement derived data, not the grid or annual series.

Current release blockers are explicit: GLO-30 tile selection, the manually
acquired HYDE 3.5 files, EGDI, and EU-Hydro lack checked complete content
inventories. The forest marker pins only the preparation format, so forest
remains non-reproducible until every consumed raster is inventoried and hashed.
SoilGrids is a rolling service whose strict prepared manifest supplies the
actual retrieval timestamp and snapshot identity; that does not make raw
`latest` reacquisition reproducible. These entries do not claim legal
resolution or reproducibility that is not present.

`scripts/world_source_init.py` provides bounded plan/init/verify workflows for
these accepted sources. Only the immutable EU-Trees4F archive is acquired; the
authenticated or incompletely pinned sources fail closed until reviewed
inventories are committed. The curated IEG CSV is validated in place and its
rights-reserved reference images are never mirrored. Sidecars use sorted fixed
metadata and actual sizes/hashes, reject unknown inventory fields, traversal,
symlinks, redirects outside fixed hosts, oversized content, and partial or
checksum-failing publication.

The EU-Trees4F initializer pins the exact JRC ENS_CLIM archive. It retains the
EU-Trees4F v2 Figshare citation and CC0 notice, but does not claim that a
Figshare-hosted archive is byte-identical; confirming that relationship remains
an explicit provenance blocker.

## Source-separated developer bundles

`scripts/world_data_bundle.py` provides a distinct distribution boundary for
development inputs. Its ZIP is a manifest-and-notices collection: source
components remain separately addressed and are installed into the compiler's
existing source paths only after complete verification. It carries no compiled
world artifact. The policy includes every active input but rejects LUH1,
rights-reserved IEG map images, and raw OWDA grid/annual data; IEG's committed
coarse CSV stays in the repository and OWDA may appear only as a bounded
per-settlement derived profile. This engineering policy is fail-closed and does
not itself resolve source-specific redistribution terms or the licence status of
a future combined world-data release. See `docs/WORLD_DATA_BUNDLES.md`.
