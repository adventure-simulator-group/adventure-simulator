# Soil world data

Settlement soil plausibility is sourced from the **European Soil Database
v2.0** (ESDB), specifically its SGDBE polygon and attribute database plus the
PTRDB pedotransfer-rules table.

- Dataset page: <https://esdac.jrc.ec.europa.eu/content/european-soil-database-v20-vector-and-attribute-data>
- SGDBE dictionary: <https://esdac.jrc.ec.europa.eu/content/sgdbe-attributes>
- PTRDB dictionary: <https://esdac.jrc.ec.europa.eu/content/ptrdb-attributes>
- Distribution schema: <https://esdac.jrc.ec.europa.eu/ESDB_Archive/ESDB_Data_Distribution/ESDB_Data_full_distribution/ESDB_data_vx.cfm>

The vector archive is not an anonymous download. It requires registration and
acceptance of ESDAC's terms. Those terms do not grant this repository a general
right to redistribute the archive or derived values and may be incompatible
with commercial use. Do not commit the source files, compiled values derived
from them, or add the download to the initialization script until the project
has obtained suitable permission. The importer and synthetic tests are source
code only; they do not include ESDAC data.

## Required extracted layout

After obtaining permission, extract the official
`soilDB_shapefiles_and_attributes.zip` archive and pass its directory with
`--soil-dir`. The default is
`target/world-data-sources/raw/soil/soilDB_shapefiles_and_attributes/`.
The importer currently requires:

- `SGDBE4_0.shp`, `.shx`, `.dbf`, and `.prj`: 1:1,000,000 soil mapping units;
- `STU_sgdbe.dbf`: soil classification and dominant parent material by STU;
- `STU_ptrdb.dbf`: inferred physical properties by STU.

The two `.access-page.html` files currently under the manually downloaded soil
directory are ESDAC access-page redirects, not ZIP archives, and cannot be
parsed as data.

## Parsing and canonical model

The source boundary requires the legacy GISCO Lambert azimuthal equal-area
projection used by SGDBE; it must not be silently interpreted as EPSG:3035.
Polygon records provide a soil mapping-unit ID, dominant soil typological-unit
ID, and dominance percentage. Attribute tables are joined by STU.

Mapped profiles retain those source IDs, an exhaustive WRB reference-group
enum, dominant parent-material code, and typed gameplay properties: substrate
and texture, depth to rock, available water capacity, topsoil organic carbon,
stone content, dominant agricultural limitation, and annual water regime.
The parser uses the physical DBF names (`WRBLV1`, `PARMADO`, and `AGLI1NNI`),
not the longer logical attribute names shown in parts of the metadata. Source
code domains are parsed exhaustively. Structurally unexpected fields or new
non-placeholder codes fail the import rather than becoming `Unknown`.

Material-dependent fields live inside substrate variants. Mineral and other
non-textured substrates carry depth, water capacity, carbon, and stone content;
organic substrate does not carry a separately contradictory carbon class; rock
outcrop carries only stone content. This prevents canonical states such as a
very deep, high-water-capacity rock outcrop.

Documented no-information markers (`0`, `#`, or blank), WRB non-soil categories
(town, water, marsh, glacier, disturbed ground, and rock outcrop), incomplete
joins, and settlements outside polygon coverage instead produce a complete
`Inferred` profile from already-typed elevation and potential vegetation. Wetland and mire
formations favor wetter or organic soils; alpine terrain favors shallow stony
or rocky soil; dry Mediterranean and steppe formations favor coarser, drier
soil; all other settlements receive a temperate medium-textured fallback.
These are intentionally plausible game-generation inputs, not historical
claims about a named settlement.

The official archive was unavailable during implementation. Normal tests build
a synthetic shapefile plus both standalone DBF tables and exercise projection,
joins, no-information handling, polygon sampling, and code domains. The ignored
`full_source_boundary_reads_registered_distribution` test is the explicit
end-to-end verification gate once an authorized archive is available:

```powershell
$env:ESDB_DIR = "C:\path\to\soilDB_shapefiles_and_attributes"
cargo test -p adventuresim-world-import full_source_boundary_reads_registered_distribution -- --ignored
```
