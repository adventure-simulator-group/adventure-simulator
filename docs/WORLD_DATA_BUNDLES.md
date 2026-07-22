# World-data input bundles

A world-data input bundle is the supported convenient development handoff:
download one reviewed ZIP, verify it, install it, then run `just compile-world`.
It is a **collection of separately addressed inputs**, not a combined derived
world artifact and not a declaration that all component licences are mutually
compatible.

```bash
just init-world-data
just verify-world-data-bundle /path/to/adventuresim-world-inputs.zip /path/to/adventuresim-world-inputs.release.json <published-descriptor-sha256>
just install-world-data /path/to/adventuresim-world-inputs.zip /path/to/adventuresim-world-inputs.release.json <published-descriptor-sha256>
just compile-world
```

`just init-world-data` is the standard developer path. It downloads the exact
full release pinned in the checked-in `world-data-release.lock.json` from the
project's public R2 development URL, resumes an interrupted byte-range download,
verifies the separately pinned descriptor, and atomically installs the
source-separated inputs. It does not download a combined `world-1544.json`.
Every full release is required to contain both the reviewed Viabundus v2
component and the four HYDE 3.5 c9 inputs (`cropland.nc`, `grazing_land.nc`,
`urban_area.nc`, and `general_files.zip`), as well as the other required source
components. Their payloads, inventories, notices, and licences remain separate
inside the collection.
The installer retains the downloaded ZIP below `target/world-data-bundle-cache/`
for later verification and requires roughly 50 GiB of free disk space while it
installs. If a different release is already installed, rerun the script with
`--replace` only after checking its recoverable backup.

`just rebuild-world-data` deliberately skips R2 and compiles a world from
already installed local inputs. It is the explicit rebuild path; it does not
re-acquire the upstream sources or overwrite the pinned release.

The installer stages and verifies every member before it changes any destination.
It refuses to merge with an existing local component. To intentionally replace
one, use `just replace-world-data /path/to/adventuresim-world-inputs.zip /path/to/adventuresim-world-inputs.release.json <published-descriptor-sha256>`; the
previous component remains recoverable below `target/world-data-backups/`.
Do not remove that backup until the new compile has succeeded.

Verification and installation require the separately published canonical release
descriptor. It contains the expected archive SHA-256 plus hashes of the
canonical manifest and component inventory; it is deliberately outside the ZIP
so a rebuilt archive cannot authenticate itself. The release maintainer uploads
both files to the central release and publishes the descriptor's SHA-256 in the
release notes through the normal project review process. The developer supplies
that independently published digest to the verify/install command; a ZIP and
descriptor that merely agree with each other are rejected without it.

## Archive contract

At the archive root, `bundle-manifest.json` is canonical JSON with a sorted
component inventory. Every payload component has a separate
`NOTICES/<source-id>.md`; its files live only below
`payload/<source-id>/`. Each component names its source/version, distribution
form, installer destination, notice, and sorted relative path/size/SHA-256
records. The verifier rejects unknown manifest fields, noncanonical order,
duplicates, extra members, unsafe paths, encrypted or symlink ZIP members,
unbounded compression, and hash/size mismatches. It intentionally permits ZIP64
when Python supports it.

Hidden local transfer fragments ending in `.part` are not source inputs and are
excluded from a build. A completed payload remains independently enumerated and
hashed in the archive manifest.

The default `build` command refuses an incomplete collection: it requires every
active compiler input, the IEG checked-in marker, reviewed layouts, and its
canonical per-component file inventory. `--partial` exists only for explicitly labelled test or
non-developer archives and must not be published as the developer release.

The Viabundus component includes only the five audited CSVs consumed by the
importer plus its official source sidecar. The sidecar may describe additional
supplementary CSVs in the upstream release; those files are intentionally not
copied into the bundle.

The policy is explicit and fail-closed for every current compiler input:
Viabundus, HYDE 3.5, GLO-30, Copernicus forest, Jung PNV, EU-Trees4F,
prepared SoilGrids, EGDI, curated IEG religion, OWDA-derived profiles, and
EU-Hydro. It rejects LUH1, IEG maps/images, and raw OWDA grids or annual series.
IEG's coarse curated CSV remains a checked-in repository asset, so the bundle
records its notice but never copies a payload over a version-controlled file.

OWDA is permitted only as a bounded, per-settlement derived profile component.
The raw NOAA NetCDF source is not eligible for the bundle. A release must use
the documented derived profile form consumed by the importer; do not substitute
the source grid merely because it is convenient.

That component contains exactly `settlement-profiles-1544.json`, installed at
`target/world-data-sources/prepared/owda/`. It is canonical JSON with
`schema: 1`, `source: "noaa-owda-v1-derived"`, `version: "1544"`, `year: 1544`,
the hashes of the matching Viabundus sidecar and settlement-ID inventory, and
sorted `profiles`. Each profile has `settlement_id`, `sampling` (`direct` or
`nearest`), `current_milli_pdsi`, `mean_milli_pdsi`, `drought_summers`, and
`wet_summers`.
It therefore carries a bounded 20-year summary for a particular settlement, not
OWDA coordinates, grid cells, or annual values.

## Building a release collection

Release maintainers prepare each source's approved raw, prepared, curated, or
per-source-derived directory independently. They must review the applicable
terms, attribution, source inventory, and every modification before publishing;
the tool does not make a legal clearance decision. It never accepts or creates a
combined `world-1544.json` input.

For example, with already reviewed local directories:

```bash
python3 scripts/world_data_bundle.py build --partial --output partial-test-inputs.zip \
  --component hyde-3-5-c9=target/world-data-sources/raw/hyde35-land-use \
  --component viabundus-v2=viabundus \
  --include-checked-in
```

The final archive should be hosted in the project’s chosen release storage with
its descriptor and descriptor checksum. Create the reviewed descriptor only
after the final archive is immutable:

```bash
python3 scripts/world_data_bundle.py describe adventuresim-world-inputs.zip \
  --output adventuresim-world-inputs.release.json
```

Source bytes are deliberately not committed to this repository.

## Publishing to the project R2 bucket

The release tool can upload a verified full ZIP and its external descriptor to
the fixed `adventuresim-world-data` Cloudflare R2 bucket. It reads the local
repository `.env` without displaying its values. The file must provide
`R2_ACCOUNT_ID`, `R2_S3_ACCESS_KEY_ID`, `R2_S3_SECRET_ACCESS_KEY`, and
`R2_S3_API_ENDPOINT`; `R2_API_TOKEN` is not used for S3 object upload. The
endpoint must be the HTTPS R2 endpoint for the stated account. Install AWS CLI
v2, which uses multipart upload for this large object, then publish only after
the release descriptor is final:

```powershell
python scripts/world_data_bundle.py publish adventuresim-world-inputs.zip `
  --descriptor adventuresim-world-inputs.release.json `
  --descriptor-sha256 <published-descriptor-sha256>
```

The command validates the ZIP and descriptor before uploading, writes the ZIP
and descriptor below `releases/world-data/`, and verifies each resulting R2
object's content length. It never uploads a partial profile. The resulting
object locations are:

```text
s3://adventuresim-world-data/releases/world-data/<archive-name>.zip
s3://adventuresim-world-data/releases/world-data/<archive-name>.release.json
```

Uploading does not make clients select the new release automatically. After
both immutable objects are publicly readable through the project's
`pub-46168a4accb04d08ad0a558b0a2abfaa.r2.dev` custom R2 development domain,
update the checked-in `world-data-release.lock.json` with their public HTTPS
URLs, the exact ZIP byte size, and the SHA-256 of the external descriptor.
`just init-world-data` downloads exactly that pinned pair; it never discovers a
"latest" object by listing the bucket.

## Future combined release

This workflow is separate from a future download of a fully compiled
`world-1544.json`. A combined output can be an adapted database and requires a
separate licence/provenance review, particularly for Viabundus’s CC BY-SA terms
and non-CC source-specific conditions. Do not infer permission for that future
release from the ability to distribute this source-separated collection.
