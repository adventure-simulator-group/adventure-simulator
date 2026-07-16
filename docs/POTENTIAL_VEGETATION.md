# Potential-natural-vegetation world data

Settlement potential vegetation comes from **EuroVegMap 2.1, Map of the
Natural Vegetation of Europe**. Unlike modern forest cover, this map describes
the vegetation expected from site conditions, making it useful for plausible
1544 reconstruction where later land conversion changed the landscape.

- Product page: <https://www.synbiosys.alterra.nl/eurovegmap/>
- Distribution version: 2.1.0
- Installer SHA-256: `6cd9d8d079cc9d86d0dceac6a88bd12878edfa7aacbd2e159240c98b9443bbad`
- Redistribution terms: no explicit licence was found in the downloaded 2.1
  distribution; keep the raw files local until terms are clarified

The official download is an Inno Setup installer. It is not part of the data
initialization script while source suitability and redistribution terms are
being evaluated. Extract or install it manually, then place its `Maps`
directory at
`target/world-data-sources/raw/potential-vegetation/Maps/`. The importer
requires `Vegetation.shp`, its DBF/index companions, and `Vegetation.prj`.
Override the location with `--potential-vegetation-dir`.

## Parsing and canonical model

The source contains 19,059 polygons at 1:2.5 million scale. Its custom
`ETRS89-LAEA5220` projection uses a Lambert azimuthal equal-area grid centered
at 20° E, 52° N with false easting 5,071,000 m and false northing 3,210,000 m.
The importer checks the exact bundled projection contract, uses pure-Rust
`proj4rs` to project settlement coordinates, and uses the pure-Rust
`shapefile` and `geo` crates for typed DBF and polygon parsing. No geospatial
dependency enters the SpacetimeDB module.

A mapped settlement stores `PotentialVegetation::Mapped`, containing a
validated `EuroVegMapUnitCode` such as `F27` and one of the source's exhaustive
top-level formations:

- polar/nival, tundra/alpine, or open woodland/subalpine;
- coniferous/mixed, deciduous/mixed, thermophilous, hygrophilous, swamp/fen,
  or floodplain forest and wetland formations;
- Atlantic heath, Mediterranean sclerophyll, xerophytic scrub, forest steppe,
  steppe, oroxerophytic vegetation, or desert;
- coastal/halophytic, aquatic/reed, or mire vegetation.

The source's lake, sea, upper-Danube support, non-European, and unkeyed salt
polygons do not pretend to be vegetation units. A settlement hitting one of
those polygons or lying outside mapped coverage receives
`PotentialVegetation::Inferred(formation)`, deterministically selected from
elevation, latitude, and typed forest cover. The enum records that distinction
without an `Unknown` variant or a fabricated source code. The build report
counts inferred samples.

The official extracted files were read successfully in full: all 19,059
records passed the source boundary, and 5,912 of the 6,041 active 1544
Viabundus settlements resolved directly to mapped vegetation. This verifies
the reader and cross-source coverage against the downloaded distribution, not
the historical truth of every coarse polygon.

Potential vegetation can guide biome and tactical scene selection, reconstruct
plausible woodland where modern cover is sparse, constrain tree-species
inference, and suggest woodland, heath, wetland, steppe, or Mediterranean
products. The detailed mapping-unit code is retained so later integrations can
join richer legend descriptions and species lists without flattening the
source prematurely.
