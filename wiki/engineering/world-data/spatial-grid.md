# Canonical spatial grid

World-data enrichment stages use one source-independent grid identity. Its CRS
is EPSG:3035 (ETRS89 / LAEA Europe), its serialized origin is exactly `(0 m,
0 m)`, and cells are square. The default cell size is 1,000 m. Deliberate
alternatives must be integer metres from 250 through 100,000, inclusive, and
divisible by 250. Alternate projections, origins, or rectangular cells are not
valid world artifacts.

`adventuresim-world-schema` owns the validated `SpatialGridSpec` wire type.
Private fields and custom deserialization prevent malformed artifacts from
bypassing its constructors. The importer owns WGS84-to-EPSG:3035 projection,
millimetre quantization, and cell assignment. Euclidean division makes negative
coordinates and exact cell boundaries deterministic. Raw TIFF, COG, vector,
and source-manifest parsing stays in individual importer source modules.

Coverage is not part of the shared grid. Each source manifest must state its
own extent and distinguish these cases explicitly:

- the requested cell is outside source coverage;
- the cell is covered but all relevant observations are nodata;
- the cell has usable observations.

Stages must document and record any fallback instead of silently mapping
outside-coverage or nodata states to zero.

## Identity and future caches

The world schema version, inference-rules version, and complete grid spec are
serialized in metadata and therefore participate in the final BLAKE3 artifact
ID. Old artifacts missing these fields fail deserialization; they are not
assigned defaults.

No intermediate cache framework exists yet. When one is introduced, every key
must contain:

1. world schema version and inference-rules version;
2. the complete serialized grid spec;
3. stage name and stage implementation/version;
4. source checksums sorted by their stable source identity.

Any mismatch means regeneration. Source coverage extents belong in the source
manifest and will affect its checksum rather than expanding `SpatialGridSpec`.
The complete canonical manifest checksum and operational-notice contract are
documented in `wiki/engineering/world-data/source-manifests.md`.
