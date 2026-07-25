# Development Workflow

## Quest content

Quest and bestiary content lives in `content/quests/*.yaml` using the same
strict JSON-compatible YAML convention as dialogue. Validate it with:

```powershell
cargo run -p adventuresim-core --bin questgen-check -- validate
```

The build reports the source file and offending ID or relation for duplicate
IDs, dangling bridge/monster references, unknown mechanics, invalid evidence
DC ranges, and zero weights without a hard-zero reason.

Local workflow for running the Adventure Simulator demo.

For deterministic multi-year NPC balance experiments and replay commands, see
[`STRATEGIC_SIMULATION.md`](STRATEGIC_SIMULATION.md), `just strategic-sim`, and
`just test-strategic-sim`. The isolated `just strategic-sim-core-loop` command
also evaluates the authoritative server-side NPC adventurer system and writes a
Markdown quest-story log. Its default policy is scripted; optional provider
mode requires explicit network consent and reads a key only from the named
environment variable. See
[`STRATEGIC_SIMULATION.md`](STRATEGIC_SIMULATION.md#quest-evaluators).
The separate end-to-end web evaluator is LLM-only and drives the same visible
controls as a player. With a local strategic server running, invoke
`just quest-web-eval quest-browser-run-001`. It saves `index.html`,
`manifest.json`, and a chronological PNG after every action. It requires
Playwright's Chromium browser (`npx playwright install chromium`) and reads the
model credential from `OPENAI_API_KEY` by default.
The opt-in authoritative integration driver is
`just strategic-sim-core-loop`; the recipe creates, claims, and deletes its own
  nonce-named loopback database, compiles a one-run bootstrap capability in
  memory, and accepts no host, database, or capability override.

## Architecture Overview

The game uses a Mount & Blade style architecture:

1. **Strategic Layer** (SpacetimeDB) - Character progression, inventory, mission tracking
2. **Tactical Spawner** - Watches for pending missions, spawns tactical servers
3. **Tactical Server** (Lightyear) - Real-time 3D gameplay server
4. **Browser Client** (WASM/Bevy) - Connects to tactical server via WebTransport

**Flow:**
1. Player clicks a location in the browser → SpacetimeDB creates a "pending" mission
2. Tactical spawner sees pending mission → starts a adventuresim-tactical-server process
3. Tactical server writes connection info to SpacetimeDB → status becomes "ready"
4. Browser polls and sees "ready" → loads WASM game and connects to tactical server
5. Mission ends → tactical server commits results and exits

## Quick Start

```bash
just dev
```

Open http://localhost:8080

Ordinary `just dev` / `just web` startup publishes without deleting database
data. Publication failures stop startup before the seed reducer, tactical
spawner, or web process starts and print the server/database identity plus
recovery choices. Canonical reset recipes are intentionally disabled.

For a disposable demo or worktree, use an explicit isolated profile:

```bash
just web-isolated renderer-demo 23100
```

The profile name is restricted to lowercase letters, digits, and hyphens, and
the base port must leave room for the web and tactical ports. The recipe derives
a stable fingerprint from the resolved worktree path and includes it in the
database name and profile directory. State lives below the current user's local
runtime/cache directory rather than shared `/tmp`; directories and metadata are
owner-only and symlinks/path escapes are rejected. Thus the same human-readable
profile in two worktrees still has distinct database, data, logs, and process
identities. The three loopback ports remain explicit, and startup fails if any
is already occupied.

The Python lifecycle process holds an exclusive profile lock from the first
port check through web-server exit. It records each child process's resolved
executable and OS creation token, checks that identity throughout readiness and
immediately before reset-publish, and uses the same identity for cleanup. It
will not treat an unrelated listener as its SpacetimeDB or signal a reused PID.
Only this guarded workflow may pass
`--delete-data=always`; it rejects remote servers, non-loopback binds, mismatched
database names, and unsafe profile strings. It stops its own SpacetimeDB and
spawner when the foreground web process exits. The isolated database files are
retained under the fingerprinted profile directory for inspection and are reset
the next time that exact worktree/profile is run.

The public `dev_stack.py publish` command is always non-destructive. Reset
publication is not a CLI option: it is an internal lifecycle operation that
requires the held profile lock and re-verifies the captured standalone listener
identity immediately before invoking SpacetimeDB.

## Full Development (with Tactical Servers)

To run the complete stack with automatic tactical server spawning:

**Terminal 1:** Start SpacetimeDB, the strategic web server, and tactical spawner
```bash
just dev
```

**Terminal 2:** (Optional) Rebuild the WASM client independently
```bash
just build-wasm
```

Now when you click a location in the browser, a tactical server will automatically spawn.

## Strategic-Only Development

For strategic-layer work, start SpacetimeDB and the server-rendered browser UI
without building the tactical WASM client or tactical server binaries and
without running the tactical dispatcher:

```bash
just dev-strategic
```

This preserves the canonical local database just like `just dev`. It also
stops a canonical dispatcher left by an earlier full-stack run. Tactical
missions can still enter the pending state, but they will not start until the
full stack is running again.

For a disposable, worktree-safe strategic-only database, use:

```bash
just web-isolated-strategic renderer-demo 23100
```

The isolated lifecycle retains the same guarded reset, ownership checks, and
cleanup as `web-isolated`, but it neither reserves the tactical port nor starts
a dispatcher.

For native tactical testing from WSL on Windows, the equivalent of running
`just dev`, `just tactical`, and `just client 0` in separate Linux terminals is:

```bash
just win-dev
```

This runs the strategic stack in WSL, cross-compiles and stages the tactical
executables in `E:\adventure-sim-dev`, then starts one native Windows tactical
server and client 0. Press Ctrl+C to stop the web process and Windows tactical
processes; the detached SpacetimeDB and tactical dispatcher follow the normal
`just dev` lifecycle and can be stopped with `just stop`. The recipe installs
the pinned toolchain's `x86_64-pc-windows-gnu` Rust target when needed; the WSL
package `gcc-mingw-w64-x86-64` must already be installed.

## Requirements

- Rustup (the repository's `rust-toolchain.toml` automatically selects the
  pinned nightly toolchain and required components)
- just (`cargo install just`)
- SpacetimeDB CLI 2.6.1:
  `curl -sSf https://install.spacetimedb.com | bash`, then
  `spacetime version install 2.6.1` and `spacetime version use 2.6.1`
- Python 3
- Node.js 20 or newer (strategic browser behavior tests)
- wasm-bindgen (`cargo install wasm-bindgen-cli`) - for WASM builds
- Caddy (for the HTTPS HTTP/2 development entry point)

The `justfile` and repository automation do not require Bash. Stateful or
compound recipes are implemented in Python, and simple recipes are compatible
with the host shell. On Windows the default interpreter is `python`; on other
platforms it is `python3`. Set `PYTHON_BIN` when the interpreter has a different
name or lives outside `PATH`.

## Services and Ports

| Service | Port | Description |
|---------|------|-------------|
| SpacetimeDB | 3000 | Strategic database |
| Strategic web | 8080 | Axum server-rendered browser UI |
| Strategic web HTTPS | 8443 | Caddy HTTP/2 and HTTP/3 entry point |
| Tactical Server | 6000+ | Game server (one per mission) |

## Core Commands

```bash
# Development
just dev              # Start the complete browser stack
just dev-strategic    # Start only SpacetimeDB and the strategic browser UI
just web-isolated     # Reset and start an explicitly isolated local profile
just web-isolated-strategic # Reset and start an isolated strategic-only profile
just web-secure       # Start strategic-web at https://localhost:8443
just secure-web-trust # Trust Caddy's local development CA (normally once)
just web-damaged      # Refuses canonical reset; seed damage in an isolated profile
just spawner          # Run tactical server spawner
just build-wasm       # Build WASM client

# Testing
just test             # Run native Rust/browser tests and validate the SpacetimeDB module ABI
just test-chat        # Run only the strategic chat behavior tests
just test-schedule    # Run only the training-schedule editor tests
just test-dev-stack   # Test local workflow policy without writing bytecode
just tactical         # Run a single tactical server (for testing)
just status           # Check service status
just stop             # Stop all services

# Workspace verification
just fmt              # Format all Rust workspace packages
just check            # Check all Rust workspace packages
just test             # Test native Rust packages and build the SpacetimeDB module
just lint             # Run Clippy with warnings denied

# Building
just build-tactical   # Build adventuresim-tactical-server and adventuresim-tactical-server-dispatcher
just build-all        # Build everything

# Database
just publish          # Publish SpacetimeDB module
just publish-reset    # Refuses canonical database deletion
just generate-db-client # Regenerate and format the Rust client bindings
just verify-db-client   # Fail if committed bindings differ from the module ABI

# World-data source
just init-world-data  # Install the pinned full input bundle, including Viabundus and HYDE
just init-world-runtime # Install the small compiled world/map runtime bundle
just verify-world-data-bundle /path/to/archive.zip /path/to/archive.release.json <published-descriptor-sha256> # Verify a reviewed input collection
just install-world-data /path/to/archive.zip /path/to/archive.release.json <published-descriptor-sha256> # Install it without source-by-source downloads
just build-base-terrain # Build documented-road-only inference terrain
just compile-world      # Build base terrain, then compile the 1544 world
just build-strategic-map # Build base, world, and final map/terrain artifacts
just normalise-viabundus # Compatibility alias for compile-world
just load-world         # Load it into a published local SpacetimeDB module
```

`just test` runs the strategic browser tests and the native Rust test suites,
excluding `adventuresim-stdb-module`. It also runs `spacetime build` to validate
that module against the SpacetimeDB host ABI. Native linking cannot provide that
host ABI, and including the module would enable its shared schema feature for
the entire workspace. Its pure strategic calculations live in
`adventuresim-core` and are covered by native unit tests. Reducer integration
tests require a running SpacetimeDB environment.

The workspace pins both the module crate and Rust SDK to SpacetimeDB 2.6.1.
Before building or generating bindings, verify the active CLI with
`spacetime --version`. Binding generation uses `spacetime generate
--module-path` and intentionally excludes private tables. Both native tactical
and WASM builds run `just verify-db-client`, which generates and formats into a
temporary directory and compares the result without changing the checkout.

The 1.x to 2.6.1 project upgrade is intentionally a clean reset: the game is
pre-launch, so existing local and deployment data is not retained. Stop the
old server, take an operator backup only if the old data may still be useful,
and move its data directory aside or configure a new empty data directory.
Then install/select SpacetimeDB 2.6.1 and use an explicitly isolated profile,
or perform a separately reviewed operator migration. `web-isolated` starts a
profile-owned server, reset-publishes, reseeds the normal world plus the sick
`Sick Demo`, injured `Wounded Demo`, and all-traditions `Religion Scholar Demo`
UI fixtures, and launches the browser stack; it
permanently discards only that profile's contents. Keep
the moved directory until the reset has been validated, then retire it under
the operator's normal backup-retention policy.

After this one-time reset, routine startup should use `just dev` / `just web`
and routine publishes should use `just publish`, all of which preserve data.
`web-reset` and `publish-reset` refuse to run. Never pass destructive publish
flags manually against a public or player-bearing database unless data loss is
explicitly approved and a verified recovery copy exists.

`bootstrap_development_world` is itself idempotent: it inserts only missing demo rows. The local
seed workflow then resets `Sick Demo` and its party of staggered patients plus a
high-Medicine physician so symptoms, diagnosis, and treatment can be tested
immediately. It propagates every reducer failure
instead of treating arbitrary errors as evidence that seeding already happened.

Individual fixture reducers are not published. The isolated profile launcher
creates a 256-bit token, compiles it into that disposable module build,
publishes, invokes the single development-bootstrap reducer, and removes the
token from child-process environments. For a manual disposable publish, set
`ADVENTURESIM_DEV_BOOTSTRAP_TOKEN` to a 64-character hexadecimal value while
publishing, then pass the same value to `scripts/dev_stack.py seed --token`.

Spawner metadata contains the resolved repository, profile, server/database,
bind/port configuration, hashes of both tactical binaries, actual executable,
PID, and OS process-creation token. Start, reuse, and stop are serialized under
the profile lifecycle lock. A live process with missing or different metadata
is rejected, so a worktree cannot silently reuse another checkout's dispatcher,
an out-of-date build, or a recycled PID. Confirmed-dead metadata is safely
replaced.

## Viabundus source data

The Viabundus v2 CSV data is a local development input for the strategic world
importer and is intentionally ignored by Git. `just init-world-data` installs
the reviewed Viabundus component together with HYDE and the other
source-separated inputs. Its component inventory records the source URLs,
sizes, and SHA-256 checksums, including `viabundus/.viabundus-source.json`.

Most developers do not need these compiler inputs at all: `just load-world`
automatically installs the separately pinned compiled runtime bundle and loads
it without rebuilding. Install the full source bundle only when changing or
auditing world generation. After the source bundle is installed (or every
source has been initialized individually), run `just build-strategic-map`. The
dependency chain first writes the immutable
`terrain-routing-base-v2.json`/`.pack`, compiles `target/world-1544.json`
against that digest, then regenerates
`target/strategic-map/strategic-map-v1.json` and the derived
`target/strategic-map/strategic-map-tiles-v1.pack`, plus the independent
`terrain-routing-v2.json`/`.pack` final native-detail artifact. The base pack is
an inference input and must not be served. The final pack adds the exact inferred
polylines from the compiled world to its road mask and records the geometry,
Jung wetland, content, and package identities. The compiler also
writes `STRATEGIC_MAP_DATA_LICENSE.md` beside every output directory. Keep that
notice with any copied, published, or hosted bundle; it is the artifact-level
licence and attribution boundary described by the repository's
`MAP_DATA_LICENSE.md`. Adjacent Jung wetland raster cells are dissolved and
their display-only boundaries are softened so source-cell seams do not appear
on the map; routing still rasterizes the exact source cells. These deterministic
presentation assets verify the initialized v2 edge and water files against
their recorded SHA-256 identities, retains only active 1544 overview roads and
ferries for presentation, and separately rasterizes every active full-precision
Viabundus road into the routing pack. It classifies installed native GLO-30
slopes and multi-scale relief, and reduces every available prepared forest tile
into bounded canopy-density and leaf-type regions. Missing forest tiles remain absent and their coverage is
reported as partial; they do not block map generation. The command clips and
simplifies presentation geometry, renders a Paper AVIF pyramid through zoom
level 3 (approximately 25 m/pixel native detail within the exact 8.965–11.110°E, 50.877–52.211°N playable bounds), concatenates the independently addressable images into one pack,
and embeds a digest
over every presentation-affecting package field. The
versioned filename is stable rather than content-addressed. Legacy Viabundus
sidecars without byte sizes remain usable for local generation but the package
marks them `legacy-release-blocked-missing-sizes`; they are not a fully verified
release snapshot. It is presentation data only: settlements still come
from the canonical strategic database and map geometry is not persisted in
SpacetimeDB. Run this command whenever the initialized Viabundus release or map
package schema changes. The elevation and forest directories default to
`target/world-data-sources/raw/elevation/` and
`target/world-data-sources/raw/forest-cover/`; use the generator's explicit
directory flags when regenerating from another reviewed installation.

The offline compiler lives in `adventuresim-world-import` behind its opt-in
`strategic-map-renderer` feature (enabled by the `just` recipe); it is not a
`strategic-web` dependency, and normal workspace builds do not select its image
renderer or encoder dependencies. At startup the server optionally loads the bundle directory from
`STRATEGIC_MAP_BUNDLE_DIR` (default `target/strategic-map`) for bounds,
attribution, indexed byte ranges, and integrity checks. If the bundle is absent
or invalid, the server still starts and the surrounding HTML destination and
direct-travel controls remain usable. Individual tile routes
include the pack's SHA-256 in their query string and receive a one-year
immutable cache policy. The browser requests only the Paper tiles covering its
current view and replaces them as it pans or crosses a zoom level. The deepest
level uses a higher AVIF quality setting for close inspection. Outside native
detail coverage, a missing deepest-level image is deterministically replaced by
the correctly cropped complete parent tile, so the map never goes blank without
inflating the pack with redundant generalized tiles. Every encoded
tile includes a four-pixel gutter so independent AVIF edges overlap cleanly;
close levels use the native terrain pack to classify 15-degree slopes and
20-percent canopy cover. Open hilly ground is light brown, forest is green, and
their overlap is dark green. The raster map has no symbolic hill or mountain
stamps; hilly terrain is communicated only by those area colours. Native
elevation remains available in the independent terrain pack for routing and
future terrain presentation. Each tile samples continuous, domain-warped
coverage fields with four-sample edge antialiasing instead of exposing source
pixels.
Historical road importance controls both
zoom visibility and line weight, and a restrained deterministic parchment
fiber/fleck layer prevents flat digital backgrounds. All procedural marks are
positioned in the zoom's global map coordinates so they remain stable across
tile gutters and repeated builds.
Settlement pins, locally issued available-quest pins, the party's active quest
pin at its issuing settlement, route availability, current location, selection,
and the computed terrain polyline to the selected destination remain in the authenticated
HTML/SVG overlay and are never cached as
part of the world asset.

The public `/map/data-license` route serves the same canonical notice compiled
into `strategic-web`. A compact `Map data licence` link overlays the map so the
attribution and reuse terms remain discoverable without restoring the old
legend or source-information block.

`strategic-web` requires an authenticated `SPACETIMEDB_TOKEN` even when the
optional terrain pack is absent. At startup it claims (or renews, using the same
identity) the singleton strategic-gateway authority and pins the loaded terrain
package digest. Keep a new database private until this registration succeeds;
the first authenticated claim establishes the trusted gateway identity.
The isolated `web-isolated` recipes read the current token from
`spacetime login show --token`, pass it only in the child process environment,
and never print or persist it. Run `spacetime login` before starting an isolated
profile. Canonical deployments must continue to provide `SPACETIMEDB_TOKEN`
through their secret manager.

For a small deterministic renderer preview without building the production
bundle, set `STRATEGIC_MAP_PREVIEW_PNG` to an output path and run the focused
`representative_paper_tile_has_deterministic_png_preview_hook` test with the
`strategic-map-renderer` feature.

The deployment manifest is schema 4 with renderer revision 9. It contains only
bounds, attribution/source metadata, coverage counts, the tile index, and
content digests; source roads, water rings, elevation cells/contours, and
forest regions stay in the offline compiler and are not shipped to
`strategic-web`. The server rejects stale renderer revisions and source URLs
that do not exactly match the compiler's reviewed HTTPS DOI sources. Paper-only
is intentional: the earlier Atlas-style option was superseded by the Paper-map
direction. Full spatial bucketing remains outside this aesthetic pass because
generation is offline and cacheable; mmap and an additional arbitrary cache
buster are likewise deferred while the validated pack remains file-backed
and content-versioned.

`just compile-world` retains active 1544 land and ferry segments, all nodes
needed to connect those segments, active settlements and their alternative
names, and typed settlement/city descriptions. The Rust world
compiler writes a deterministic, schema-versioned artifact to
`target/world-1544.json`, validates its references and invariants, and emits a
build report. Canonical nodes, edges, and settlements include a bounded,
unstructured Markdown `sources` field for future debugging; it is persisted but
not currently displayed. `just load-world` first verifies or downloads the
pinned approximately 60 MiB runtime archive when its files are absent, then
sends `target/world-1544.json` in bounded batches to a published local module.
The same archive installs the AVIF map and final terrain-routing package, so a
fresh checkout does not need the 26 GiB source bundle or a geospatial rebuild.
Run it after
`just publish-reset`, without `_seed-world`, when using the historical world.
Interrupted loads can be resumed without recompiling by loading the identical
artifact directly:

```powershell
cargo run --package adventuresim-world-import --bin adventuresim-world-import -- --input target/world-1544.json --load --server http://localhost:3000 --database adventuresim
```

The module rejects a different artifact or any additional batches after
completion; use `just publish-reset` before loading changed source data or a
different year.

The compiler defaults to the canonical 1,000 m spatial grid. Pass
`--grid-cell-size-meters 250` (or another multiple of 250 from 250 through
100,000) to deliberately build a different grid identity. See
`docs/SPATIAL_GRID.md`; changing the size changes the artifact ID and therefore
requires the normal explicit import reset when another artifact is active.

The loader claims a one-time import identity before sending batches. For a
production deployment, the operator must make that first call before allowing
untrusted clients to connect.

### Remaining source initializer workflows

### Source-separated development bundle

Most developers should use the reviewed one-archive input workflow rather than
collecting each upstream source. Download the archive, run
`just verify-world-data-bundle /path/to/archive.zip /path/to/archive.release.json <published-descriptor-sha256>`, then
`just install-world-data /path/to/archive.zip /path/to/archive.release.json <published-descriptor-sha256>`, and finally `just compile-world`.
The installer does not merge with existing inputs; use `just replace-world-data`
only when retaining its backup is acceptable. See
[`docs/WORLD_DATA_BUNDLES.md`](WORLD_DATA_BUNDLES.md) for the separately marked
licence collection, its OWDA/IEG boundaries, release-maintainer requirements,
and the distinction from a future combined derived-world release.

Most remaining accepted source workflows share `scripts/world_source_init.py`.
Every source has `plan-*`, `init-*`, and `verify-*` targets. EU-Trees4F is an
automated anonymous immutable download (`tree-species`); it is size/hash
checked and atomically published. Copernicus forest uses the dedicated
`scripts/init_forest_cover.py` workflow: it reads redacted Sentinel Hub OAuth
credentials from `.env`, prepares the official 2018 TCD/DLT playable-area
coverage, resumes interrupted requests, and atomically publishes an exact
size/SHA-256 inventory. Religion is a validation-only workflow and never
mirrors the rights-reserved IEG images. GLO-30 (`glo30`) and EU-Hydro
(`hydrology`) perform redacted credential-file preflights but refuse network
acquisition until exact product inventories are committed. HYDE 3.5 and EGDI likewise provide
deterministic plans and strict local-inventory verification while remaining
release-blocked. `init-*` never turns missing pins or rights restrictions into
guesses. See each source document for its exact blocker and command names.

The compiler also currently requires manually downloaded Copernicus DEM
GLO-30 `*_DEM.tif` tiles in
`target/world-data-sources/raw/elevation/`. See `docs/ELEVATION.md` for source,
licensing, parsing, and fallback details. You can override either input with
`--viabundus-dir` or `--elevation-dir`.

Historical land use uses the HYDE 3.5 c9 NetCDF area files described in
`docs/HISTORICAL_LAND_USE.md`. Obtain the three CC BY 3.0 NetCDF files and
the shared general-files archive, place them under
`target/world-data-sources/raw/hyde35-land-use/`, then run `just verify-hyde35`.
The stacked compiler requires them; override that directory with
`--land-use-dir`.

Initialize the paired Copernicus TCD/DLT one-degree GeoTIFFs documented in
`docs/FOREST_COVER.md` with `just init-forest-cover`. The default 12-tile
playable-area set is written under
`target/world-data-sources/raw/forest-cover/`; the world compiler can override
that directory with `--forest-cover-dir`.

Initialize the CC BY 4.0 Jung/IIASA European PNV v1.1 rasters with
`just init-jung-pnv`. Verified local files are written under
`target/world-data-sources/raw/jung-pnv/`, as documented in
`docs/POTENTIAL_VEGETATION.md`. Override it with `--potential-vegetation-dir`.

EU-Trees4F v2 tree-species suitability is read directly from the downloaded
`target/world-data-sources/raw/tree-species/EU-Trees4F_ens-clim.zip`, as
documented in `docs/TREE_SPECIES.md`. Override it with
`--tree-species-archive`. This stage requires all 67 current-climate
probability, potential, and native-range rasters in the pinned archive.

SoilGrids preparation is explicit because the fixed European subset contains
hundreds of layers and requires GDAL. `just init-soilgrids` prints the bounded
plan. Run `python scripts/init_soilgrids.py --prepare`, then `--verify-only`,
and pass a non-default prepared directory with `--soilgrids-dir`. See
`docs/SOIL.md`. A complete official-source audit remains blocked until the
prepared subset exists locally.

EGDI surface geology is read from the indexed EPSG:3034 GeoPackage at
`target/world-data-sources/raw/geology/GeologicUnitView.gpkg`, as documented in
`docs/GEOLOGY.md`. Override it with `--geology-geopackage`. The downloaded
675 MB file and a real spatial sample have been verified; it remains a manually
prepared input until the integration is accepted.

IEG official-religion maps require a curated geographic intermediate because
the published 1500 and 1555 images do not contain machine-readable territory
boundaries. The checked-in 1544 intermediate and its intentionally approximate
interpretation are documented in `docs/RELIGION.md`; override its location with
`--religion-regions`.

The bundle workflow installs only the bounded, per-settlement OWDA derived
profiles at `target/world-data-sources/prepared/owda/settlement-profiles-1544.json`;
this is the compiler default. `just init-owda` remains available for a direct
source audit of the pinned NOAA OWDA v1.0 NetCDF-4 file at
`target/world-data-sources/raw/climate/owda.nc`, but it is not eligible for
redistribution and must be passed explicitly with `--drought-netcdf`. Its strict
source boundary, typed twenty-year profile, spatial fallback, redistribution
boundary, and gameplay scope are in `docs/DROUGHT.md`.

Copernicus EU-Hydro v1.3 is read from extracted EPSG:3035 basin GeoPackages
under `target/world-data-sources/raw/hydrology/`, as documented in
`docs/HYDROLOGY.md`. Override that directory with `--hydrology-dir`. The
official archive is not currently available locally, so the parser and
enrichment are verified against standards-compliant synthetic GeoPackages but
the complete source distribution remains unverified.

The same initialized GLO-30 and EU-Hydro directories feed the final strategic
route-terrain stage. Its straight-line geometry, deterministic v5 rules, schema
bounds, and remaining authenticated-source audit blockers are documented in
`docs/ROUTE_TERRAIN.md`.

Rules-v6 settlement industries are generated immediately after route terrain.
Run `cargo test -p adventuresim-world-import sources::industries` for the
deterministic synthetic evidence matrix. The complete official-world audit
still requires every upstream distribution. See `docs/INDUSTRIES.md`.

World schema v18 / inference rules v4 add typed canonical distribution
manifests and a deterministic schema/rules/year/grid/source digest. The world
compiler prints the sorted manifest and whether each source is reproducible.
See `docs/SOURCE_MANIFESTS.md`; manual, rolling, credential-gated, or
rights-conflicted sources remain explicit instead of receiving invented hashes
or legal conclusions.

World schema v17 / inference rules v4 add the final 1544 environmental
synthesis. Its direct/derived/fallback and tie-break counters must reconcile to
settlement count during validation. A complete official all-source audit is not
available until the HYDE 3.5, forest, SoilGrids, and EU-Hydro inputs above exist
locally; unit tests use bounded synthetic evidence and make no coverage claim.

## Strategic UI

The strategic UI is server-rendered by `crates/strategic-web`. Browser clients
receive live state through the web server rather than connecting directly to
SpacetimeDB. The tactical WASM page remains under
`crates/adventuresim-stdb-module/static/tactical.html` and is served by
`strategic-web` at `/tactical/tactical.html`.

Character-sheet action menus follow one interaction contract. Raised,
old-school beveled icon buttons open modal dialogs; flat skill icons and meters
are informational. Surgery buttons sit beside limb headings, Social sits beside
Morale, and Medicine and Cooking use their skill icons. Activity icons use the
same raised treatment. An inset button means its dialog is open. Dialogs retain
the underlying rails, lock page scrolling, trap focus, close with Escape, and
return focus to their launcher. Portrait hover controls remain reserved for
inventory, membership, alchemy, and other portrait-specific actions.

The local strategic UI is anonymous and single-user. Its cookie selects the
active character; it does not establish a user identity. The default
`127.0.0.1:8080` bind is therefore intentional. A non-loopback development bind
must set `ALLOW_INSECURE_NON_LOOPBACK_BIND=true` and must remain on an isolated,
trusted network.

Test the server-rendered strategic browser through `https://localhost:8443`
using `just web-secure`. Caddy terminates TLS and negotiates HTTP/2 or HTTP/3
with the browser, then proxies to `strategic-web` on `127.0.0.1:8080`. Run
`just secure-web-trust` once if the browser does not yet trust Caddy's local
certificate authority. Port 8080 remains available for backend diagnostics but
does not exercise multiplexed browser transport.

The certificate-trust recipe runs with PowerShell on Windows even though most
of the development stack currently expects Bash. If Caddy is not on `PATH`, set
`CADDY_BIN` to the executable's full path before invoking it, for example:

```powershell
$env:CADDY_BIN = "C:\tools\caddy.exe"
just secure-web-trust
```

## Tactical Spawner

The spawner polls SpacetimeDB for "pending" missions and starts adventuresim-tactical-server processes:

```bash
just spawner
```

Each tactical server:
1. Starts on an available port
2. Calls `tactical_server_ready` reducer with connection info
3. Runs for 2 minutes (or until mission ends)
4. Calls `commit_mission` reducer with results
5. Exits

## Testing a Single Server

For testing without the spawner:

```bash
just tactical mission_id="test-123" scene_key="town_a"
```

## Troubleshooting

- **SpacetimeDB not running:** `just status`, then `just spacetime-start`
- **SpacetimeDB failed to start:** check
  `adventure-simulator-1/spacetime.log` below the platform temporary directory
  (`%TEMP%` on Windows and usually `$TMPDIR` or `/tmp` elsewhere).
- **Tactical spawner can't find binary:** run `just build-tactical` first
- **Mission stuck on "pending":** spawner not running or binary not found
- **Cargo cannot create a temporary `target` directory:** ensure the parent of
  `CARGO_TARGET_DIR` is writable. On Windows or in a restricted sandbox, use a
  workspace-local directory, for example:

  ```powershell
  $env:CARGO_TARGET_DIR = "$PWD\target\verification"
  just test
  ```

`strategic-web` logs every HTTP request at `info` level with a request ID,
method, URI, response status, and elapsed milliseconds. The same request ID is
returned in the `X-Request-Id` response header for correlation with the browser
network panel. Requests abandoned by a navigating browser are logged as
canceled rather than silently disappearing. Set
`RUST_LOG=strategic_web=info` if a shell-level log filter suppresses these
diagnostics.
# Social panel demo

Start the isolated strategic stack with the guarded visual fixtures:

```powershell
just web-isolated-strategic social-demo 23100
```

Select **Social Demo**, open **Greta the Guard**, and press the raised Social
icon beside the Morale meter.
The fixture includes defeat and injury penalties, established Familiarity,
positive Affinity, and one deliberately incorrect perceived sensitivity so the
privacy boundary and outcome rules are visible. The bootstrap capability is
compiled only for the isolated workflow; there is no standalone public fixture
reducer. Schema changes are destructive in this pre-launch workflow, so rerun
the isolated profile to recreate its database.
