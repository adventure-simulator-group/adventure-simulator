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

- `LUHa_u2.v1_gcrop.nc4` — cropland
- `LUHa_u2.v1_gpast.nc4` — pasture
- `LUHa_u2.v1_gurbn.nc4` — urban land
- `LUHa_u2.v1_gothr.nc4` — primary land
- `LUHa_u2.v1_gsecd.nc4` — secondary land

Use `--land-use-dir` to select another directory. No downloader is included:
the repository deliberately does not claim an unverified archive URL, file
inventory, or checksum.

The importer requires each file to be NetCDF-4 and to expose its named,
floating-point state variable using one `time`, one `lat`, and one `lon`
dimension. It verifies that all five coordinate/time axes are identical,
selects the requested **annual** world year directly (there is no
interpolation), reads the full annual slice for numeric validation, then
samples each settlement at its containing coordinate-cell footprint.

At each valid terrestrial cell the importer maps `gcrop`, `gpast`, `gurbn`,
and `gothr + gsecd` respectively to cropland, grazing, built-up, and
natural/seminatural fractions. The five source states must be exhaustive. Only
a tiny floating-point overfill is normalized; malformed, missing, nonnumeric,
or materially non-exhaustive source values fail the build. A cell whose state
values are all nodata or all zero is a documented deterministic fallback, and
is counted in the build report. A missing or invalid global source file is
never converted into a fallback.

Canonical land use is stored as bounded basis-point fractions for cropland,
grazing land, built-up land, and natural/seminatural land. The four fractions
sum to exactly 10,000. The profile supports agricultural production,
grazing/livestock products, cultivated-versus-wild encounters, and adjustment
of modern forest-cover fallbacks.
