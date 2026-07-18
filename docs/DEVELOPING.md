# Development Workflow

Local workflow for running the Adventure Simulator demo.

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
a distinct database name, SpacetimeDB data directory, run/PID directory, and
three loopback ports from those values. Only this guarded workflow may pass
`--delete-data=always`; it rejects remote servers, non-loopback binds, mismatched
database names, and unsafe profile strings. It stops its own SpacetimeDB and
spawner when the foreground web process exits.

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
just web-isolated     # Reset and start an explicitly isolated local profile
just web-secure       # Start strategic-web at https://localhost:8443
just secure-web-trust # Trust Caddy's local development CA (normally once)
just web-damaged      # Refuses canonical reset; seed damage in an isolated profile
just spawner          # Run tactical server spawner
just build-wasm       # Build WASM client

# Testing
just test             # Run native Rust/browser tests and validate the SpacetimeDB module ABI
just test-chat        # Run only the strategic chat behavior tests
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
just init-viabundus   # Download Viabundus v2 CSV data into viabundus/
just compile-world      # Compile initialized sources into the 1544 world in target/
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
profile-owned server, reset-publishes, reseeds, and launches the browser stack;
it permanently discards only that profile's contents. Keep
the moved directory until the reset has been validated, then retire it under
the operator's normal backup-retention policy.

After this one-time reset, routine startup should use `just dev` / `just web`
and routine publishes should use `just publish`, all of which preserve data.
`web-reset` and `publish-reset` refuse to run. Never pass destructive publish
flags manually against a public or player-bearing database unless data loss is
explicitly approved and a verified recovery copy exists.

`seed_world` is itself idempotent: it inserts only missing demo rows. The local
workflow now propagates every reducer failure instead of treating arbitrary
errors as evidence that seeding already happened.

Spawner PID files have a sidecar identity containing the resolved repository,
profile, server/database, bind/port configuration, and hashes of both tactical
binaries. A live PID with a missing or different identity is rejected, so a
worktree cannot silently reuse another checkout's dispatcher or an out-of-date
build. Dead PIDs are cleaned and replaced.

## Viabundus source data

The Viabundus v2 CSV download is a local development input for the strategic
world importer. It is intentionally ignored by Git. Initialise or restore it
from the official Zenodo record with:

```bash
just init-viabundus
```

Use `python3 scripts/init_viabundus.py --force` only when replacing an existing
local download. The command records the source URLs and SHA-256 checksums in
`viabundus/.viabundus-source.json`.

`just compile-world` retains active 1544 land and ferry segments, all nodes
needed to connect those segments, active settlements and their alternative
names, and typed settlement/city descriptions. The Rust world
compiler writes a deterministic, schema-versioned artifact to
`target/world-1544.json`, validates its references and invariants, and emits a
build report. Canonical nodes, edges, and settlements include a bounded,
unstructured Markdown `sources` field for future debugging; it is persisted but
not currently displayed. `just load-world` sends that same compiled
data in bounded batches to a published local module. Run it after
`just publish-reset`, without `_seed-world`, when using the historical world.
Interrupted loads can be resumed with the identical compiled artifact. The
module rejects a different artifact or any additional batches after completion;
use `just publish-reset` before loading changed source data or a different year.

The compiler defaults to the canonical 1,000 m spatial grid. Pass
`--grid-cell-size-meters 250` (or another multiple of 250 from 250 through
100,000) to deliberately build a different grid identity. See
`docs/SPATIAL_GRID.md`; changing the size changes the artifact ID and therefore
requires the normal explicit import reset when another artifact is active.

The loader claims a one-time import identity before sending batches. For a
production deployment, the operator must make that first call before allowing
untrusted clients to connect.

### Remaining source initializer workflows

The remaining accepted source workflows share `scripts/world_source_init.py`.
Every source has `plan-*`, `init-*`, and `verify-*` targets. EU-Trees4F is the
only newly automated anonymous immutable download (`tree-species`); it is
size/hash checked and atomically published. Religion is a validation-only
workflow and never mirrors the rights-reserved IEG images. GLO-30
(`glo30`), Copernicus forest (`forest-cover`), and EU-Hydro (`hydrology`)
perform redacted credential-file preflights but refuse network acquisition
until exact product inventories are committed. HYDE and EGDI likewise provide
deterministic plans and strict local-inventory verification while remaining
release-blocked. `init-*` never turns missing pins or conflicting rights into
guesses. See each source document for its exact blocker and command names.

The compiler also currently requires manually downloaded Copernicus DEM
GLO-30 `*_DEM.tif` tiles in
`target/world-data-sources/raw/elevation/`. See `docs/ELEVATION.md` for source,
licensing, parsing, and fallback details. You can override either input with
`--viabundus-dir` or `--elevation-dir`.

Historical land use currently has a tested parser but no accessible full local
dataset. Preparing the seven corrected HYDE 3.2.1 ESRI ASCII files documented in
`docs/HISTORICAL_LAND_USE.md` under
`target/world-data-sources/raw/historical-land-use/` is required before the
stacked compiler can complete. Override that directory with `--land-use-dir`.

Forest cover likewise has a tested boundary but no authenticated full local
download. Prepare the paired Copernicus TCD/DLT one-degree GeoTIFFs documented
in `docs/FOREST_COVER.md` under
`target/world-data-sources/raw/forest-cover/`. Override that directory with
`--forest-cover-dir`.

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

Run `just init-owda` to download or verify the pinned NOAA OWDA v1.0 NetCDF-4
file at `target/world-data-sources/raw/climate/owda.nc`. The initializer checks
the exact 228226363-byte size and SHA-256 and records both the dataset and paper
DOIs in ignored adjacent metadata. Its strict source boundary, typed
twenty-year profile, spatial fallback, redistribution boundary, and gameplay
scope are in `docs/DROUGHT.md`; override the compiler path with
`--drought-netcdf`.

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
available until the HYDE, forest, SoilGrids, and EU-Hydro inputs above exist
locally; unit tests use bounded synthetic evidence and make no coverage claim.

## Strategic UI

The strategic UI is server-rendered by `crates/strategic-web`. Browser clients
receive live state through the web server rather than connecting directly to
SpacetimeDB. The tactical WASM page remains under
`crates/adventuresim-stdb-module/static/tactical.html` and is served by
`strategic-web` at `/tactical/tactical.html`.

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
- **SpacetimeDB failed to start:** check `/tmp/adventure-simulator-1/spacetime.log`
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
