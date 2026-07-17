# Historical land-use world data

The world compiler derives its historical land-use profile from **LUH1:
Harmonized Global Land Use for Years 1500-2100, V1**, specifically the
urban-inclusive `LUHa_u2.v1` historical state product. LUH1 is a modelled,
global 0.5-degree reconstruction: it is useful regional evidence, not an exact
observation of an individual settlement.

- Dataset catalogue and citation: <https://doi.org/10.3334/ORNLDAAC/1248>
- Project archive and product README: <https://luh.umd.edu/data.shtml>
- Method paper: <https://doi.org/10.1007/s10584-011-0153-2>

The catalogue asks users to cite the dataset. This repository does not claim a
redistributable licence for the source files; obtain them through the official
LUH1/ORNL route and follow its current terms.

## Manual preparation contract

Place exactly these upstream NetCDF-4 state files in the Git-ignored
`target/world-data-sources/raw/luh1-land-use/` directory:

- `LUHa_u2.v1_gcrop.nc4` / `prop_crop` — cropland
- `LUHa_u2.v1_gpast.nc4` / `prop_past` — pasture
- `LUHa_u2.v1_gurbn.nc4` / `prop_urbn` — urban land
- `LUHa_u2.v1_gothr.nc4` / `prop_primary` — primary land
- `LUHa_u2.v1_gsecd.nc4` / `prop_secd` — secondary land

Use `--land-use-dir` to select another directory. No downloader is included:
the repository deliberately does not claim an unverified archive URL, file
inventory, or checksum.

The importer requires each file to be NetCDF-4 and to expose the listed
floating-point state variable—the variable name is deliberately not inferred
from its filename—using one `time`, one `lat`, and one `lon` dimension. It
verifies that all five coordinate/time axes are identical, selects the
requested **annual** world year directly (there is no interpolation), reads the
full annual slice for numeric validation, then samples each settlement at its
containing coordinate-cell footprint. Time may be stored as direct calendar
years (with no units, `year`, or `years`) or as `years since YYYY-01-01` with a
standard/proleptic-Gregorian calendar; other time encodings fail closed.

At each valid terrestrial cell the importer maps `prop_crop`, `prop_past`,
`prop_urbn`, and `prop_primary + prop_secd` respectively to cropland, grazing,
built-up, and natural/seminatural fractions. LUH1 separately supplies ice/water
coverage (`gicew`), which is deliberately not part of this five-file
preparation contract. Therefore the importer treats the positive five-state
total as a **conditional terrestrial** composition and normalizes it into the
exhaustive game profile. A total above one is accepted only within a tiny
floating-point tolerance; material overfill, partial nodata, or nonnumeric
values fail the build. Only a cell whose five state values are all nodata or all
zero receives the documented deterministic fallback, and is counted in the
build report. The profile intentionally does not represent the absolute
ice/water share of a source cell. A missing or invalid global source file is
never converted into a fallback.

Canonical land use is stored as bounded basis-point fractions for cropland,
grazing land, built-up land, and natural/seminatural land. The four fractions
sum to exactly 10,000. The profile supports agricultural production,
grazing/livestock products, cultivated-versus-wild encounters, and adjustment
of modern forest-cover fallbacks.
