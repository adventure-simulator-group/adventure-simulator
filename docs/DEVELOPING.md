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
data. Use `just web-reset` only when an intentional reset is required.

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
just web-reset        # Delete, reseed, and start the disposable browser stack
just web-secure       # Start strategic-web at https://localhost:8443
just secure-web-trust # Trust Caddy's local development CA (normally once)
just web-damaged      # Start a fresh stack with an injured demo character
just spawner          # Run tactical server spawner
just build-wasm       # Build WASM client

# Testing
just test             # Run the Rust workspace and strategic browser behavior tests
just test-chat        # Run only the strategic chat behavior tests
just tactical         # Run a single tactical server (for testing)
just status           # Check service status
just stop             # Stop all services

# Workspace verification
just fmt              # Format all Rust workspace packages
just check            # Check all Rust workspace packages
just test             # Test all Rust workspace packages
just lint             # Run Clippy with warnings denied

# Building
just build-tactical   # Build adventuresim-tactical-server and adventuresim-tactical-server-dispatcher
just build-all        # Build everything

# Database
just publish          # Publish SpacetimeDB module
just publish-reset    # Publish and clear database
just generate-db-client # Regenerate and format the Rust client bindings

# World-data source
just init-viabundus   # Download Viabundus v2 CSV data into viabundus/
just compile-world      # Compile initialized sources into the 1544 world in target/
just normalise-viabundus # Compatibility alias for compile-world
just load-world         # Load it into a published local SpacetimeDB module
```

`just test` runs the native test suites across the workspace. The
SpacetimeDB module itself targets the SpacetimeDB host ABI, so validate that
crate with `spacetime build`; its pure strategic calculations live in
`adventuresim-core` and are covered by native unit tests. Reducer integration
tests require a running SpacetimeDB environment.

The workspace pins both the module crate and Rust SDK to SpacetimeDB 2.6.1.
Before building or generating bindings, verify the active CLI with
`spacetime --version`. Binding generation uses `spacetime generate
--module-path` and intentionally excludes private tables.

The 1.x to 2.6.1 project upgrade is intentionally a clean reset: the game is
pre-launch, so existing local and deployment data is not retained. Stop the
old server, take an operator backup only if the old data may still be useful,
and move its data directory aside or configure a new empty data directory.
Then install/select SpacetimeDB 2.6.1 and run `just web-reset`. That explicit
startup path starts the 2.6.1 server, reset-publishes, reseeds, and launches the
browser stack; it permanently discards the database's previous contents. Keep
the moved directory until the reset has been validated, then retire it under
the operator's normal backup-retention policy.

After this one-time reset, routine startup should use `just dev` / `just web`
and routine publishes should use `just publish`, all of which preserve data.
Do not use `web-reset` or `publish-reset` on a future public or player-bearing
database unless data loss is explicitly approved and a verified recovery copy
exists.

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
needed to connect those segments, and active settlements. The Rust world
compiler writes a deterministic, schema-versioned artifact to
`target/world-1544.json`, validates its references and invariants, and emits a
build report. `just load-world` sends that same compiled
data in bounded batches to a published local module. Run it after
`just publish-reset`, without `_seed-world`, when using the historical world.
Interrupted loads can be resumed with the identical compiled artifact. The
module rejects a different artifact or any additional batches after completion;
use `just publish-reset` before loading changed source data or a different year.

The loader claims a one-time import identity before sending batches. For a
production deployment, the operator must make that first call before allowing
untrusted clients to connect.

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

EuroVegMap potential vegetation is distributed in an installer. Extract or
install version 2.1 manually and place its `Maps` directory under
`target/world-data-sources/raw/potential-vegetation/Maps/`, as documented in
`docs/POTENTIAL_VEGETATION.md`. Override it with
`--potential-vegetation-dir`.

EU-Trees4F v2 tree-species suitability is read directly from the downloaded
`target/world-data-sources/raw/tree-species/EU-Trees4F_ens-clim.zip`, as
documented in `docs/TREE_SPECIES.md`. Override it with
`--tree-species-archive`. This stage requires all 67 current-climate
probability, potential, and native-range rasters in the pinned archive.

European Soil Database v2 vector data requires ESDAC registration and
project-specific permission; it is not redistributed by this repository.
After authorization, extract the required SGDBE/PTRDB files under
`target/world-data-sources/raw/soil/soilDB_shapefiles_and_attributes/`, as
documented in `docs/SOIL.md`. Override that directory with `--soil-dir`.
Until the official archive is available, only the synthetic source boundary is
verified and the stacked compiler will stop at this stage.

EGDI surface geology is read from the indexed EPSG:3034 GeoPackage at
`target/world-data-sources/raw/geology/GeologicUnitView.gpkg`, as documented in
`docs/GEOLOGY.md`. Override it with `--geology-geopackage`. The downloaded
675 MB file and a real spatial sample have been verified; it remains a manually
prepared input until the integration is accepted.

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
