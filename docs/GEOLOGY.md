# Surface geology

Settlement surface geology is sourced from the **EGDI 1:1 Million
pan-European Surface Geology** dataset (`EGDI-GE-1M-SURFACE`, created
2016-05-04). It aggregates national geological-survey data using INSPIRE and
OneGeology lithology and geochronology codelists.

- Metadata: <https://metadata.europe-geology.eu/record/full/5729ffdf-2558-48fc-a5d2-645a0a010855>
- WFS catalogue: <https://maps.europe-geology.eu/wfs/>
- License: Creative Commons Attribution 4.0. The metadata includes an
  additional attribution/disclaimer for the Maltese contribution.
- Scale: 1:1,000,000.
- Compiler input: GeoPackage, EPSG:3034 (`ETRS89-extended / LCC Europe`).

## Preparing the source

Place the exported `GeologicUnitView.gpkg` at
`target/world-data-sources/raw/geology/GeologicUnitView.gpkg`. The world
compiler reads it directly; `--geology-geopackage` overrides the path. The
checked source boundary requires the `GeologicUnitView` feature table, its
`geom` polygonal geometry column, EPSG:3034 metadata, and the
`rtree_GeologicUnitView_geom` spatial index.

The official WFS can return GeoJSON samples, but the full layer contains more
than 240,000 features. Preparing a local indexed GeoPackage avoids repeatedly
downloading or scanning the service. This manual preparation is intentionally
not part of data initialization until the source is accepted.

## Imported model

For every settlement the importer projects its coordinates into EPSG:3034,
queries the GeoPackage R-tree, and performs an exact point-in-polygon test on
the candidate GeoPackage geometries. It retains a bounded geologic-unit
identifier and reduces the source codelists to typed gameplay categories:

- unconsolidated deposits such as clay, sand, gravel, till, peat, alluvium,
  loess, and volcanic ash;
- sedimentary rock such as limestone, chalk, sandstone, shale, evaporite,
  coal, and chert;
- igneous rock such as granite, basalt, gabbro, rhyolite, and tuff;
- metamorphic rock such as slate, schist, gneiss, quartzite, and marble;
- mixed rock, breccia, and mélange.

Geologic age is reduced to broad eras from Quaternary through Precambrian.
Lithology and age each independently record whether they came from the EGDI
attribute or from a deterministic inference. Consequently an EGDI polygon
whose age is the source marker `unknown` remains a mapped unit with an inferred
age; canonical data never stores an `Unknown` variant.

Unrecognized but present lithology terms become typed mixed rock. A missing
lithology becomes plausible sandstone. Missing ages are inferred from
lithology: unconsolidated deposits are Quaternary, coal is Carboniferous,
other sedimentary rock is Jurassic, crystalline igneous/metamorphic rock is
Precambrian, and mixed rock is Paleogene. Settlements outside source coverage
receive a complete inferred setting based first on their imported soil, with
sandstone as the general fallback.

These categories provide quarry/building-stone, clay, chalk, slate, salt,
coal, cave/karst, mining, architecture, and tactical-material priors without
claiming that every plausible resource was historically exploited.

## Verification

Parser tests create a small real GeoPackage boundary, including the application
ID, geometry metadata, spatial index, and GeoPackage binary multipolygon. An
ignored test can verify a full manual download:

```powershell
$env:EGDI_GEOPACKAGE = "C:\path\to\GeologicUnitView.gpkg"
cargo test -p adventuresim-world-import samples_downloaded_egdi_geopackage -- --ignored
```

The downloaded 675 MB file was verified with 243,092 feature rows, its EPSG:3034
metadata, spatial index, and a real mapped sample.
